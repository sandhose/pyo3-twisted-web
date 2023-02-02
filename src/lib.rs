// Copyright 2022 Quentin Gliech
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![forbid(unsafe_code)]
#![deny(clippy::all, rustdoc::all)]
#![warn(clippy::pedantic)]
#![doc = include_str!("../README.md")]

use bytes::{Buf, BufMut, Bytes, BytesMut};
use http::{header::HeaderName, HeaderValue, Method, Request, Response, Uri};
use http_body::{combinators::UnsyncBoxBody, Body, Full};
use pyo3::{
    exceptions::PyValueError,
    pyclass, pymethods,
    types::{PyBytes, PySequence, PyTuple},
    IntoPy, Py, PyAny, PyErr, PyResult, Python,
};
use tower::{
    util::{BoxCloneService, Oneshot},
    Service, ServiceExt,
};

mod util;

use self::util::BoxBuf;

/// A Resource python class which implements ``twisted.web.resource.IResource`` from a [`Service`].
/// It doesn't have a Python constructor, so it is expected to be either subclassed or returned
/// from a Rust function:
///
/// ```rust
/// use std::convert::Infallible;
///
/// use bytes::Bytes;
/// use http::{Request, Response};
/// use pyo3::prelude::*;
///
/// use pyo3_twisted_web::Resource;
///
/// #[pyclass(extends=Resource)]
/// struct MyResource;
///
/// #[pymethods]
/// impl MyResource {
///     #[new]
///     fn new() -> (Self, Resource) {
///         let service = tower::service_fn(|_request: Request<_>| async move {
///             let response = Response::new(String::from("hello"));
///             Ok(response)
///         });
///
///         let super_ = Resource::from_service::<_, _, Infallible>(service);
///         (Self, super_)
///     }
/// }
///
/// #[pyfunction]
/// fn my_resource(py: Python) -> PyResult<Py<Resource>> {
///     let service = tower::service_fn(|_request: Request<_>| async move {
///         let response = Response::new(String::from("hello"));
///         Ok(response)
///     });
///
///     Py::new(py, Resource::from_service::<_, _, Infallible>(service))
/// }
/// ```
#[pyclass(subclass)]
pub struct Resource {
    service: BoxCloneService<Request<Full<Bytes>>, Response<UnsyncBoxBody<BoxBuf, PyErr>>, PyErr>,
}

impl Resource {
    pub fn from_service<S, B, E>(service: S) -> Self
    where
        S: Service<Request<Full<Bytes>>, Response = Response<B>, Error = E>
            + Clone
            + Send
            + 'static,
        S::Future: Send,
        B: Body + Send + 'static,
        B::Data: Buf,
        B::Error: Into<PyErr>,
        E: Into<PyErr> + 'static,
    {
        let service = service
            .map_response(|response: Response<B>| {
                response.map(|body| {
                    let body = body.map_data(BoxBuf::new).map_err(Into::into);

                    UnsyncBoxBody::new(body)
                })
            })
            .map_err(Into::into)
            .boxed_clone();

        Self { service }
    }
}

#[pymethods]
impl Resource {
    #[setter]
    fn server(&self, _server: &PyAny) {
        let _ = self;
    }

    #[getter(isLeaf)]
    fn is_leaf(&self) -> bool {
        let _ = self;
        true
    }

    fn render<'a>(&self, py: Python<'a>, request: &'a PyAny) -> PyResult<&'a PyAny> {
        let service = self.service.clone();
        let not_done_yet = py.import("twisted.web.server")?.getattr("NOT_DONE_YET")?;
        let defer = py.import("twisted.internet.defer")?;
        let future = handle_twisted_request_through_service(service, request)?;
        let deferred = defer
            .getattr("Deferred")?
            .call_method1("fromFuture", (future,))?;
        defer.getattr("ensureDeferred")?.call1((deferred,))?;
        Ok(not_done_yet)
    }
}

/// Read a file-like Python object by chunks
///
/// # Errors
///
/// Returns an error if calling the ``read`` on the Python object failed
pub fn read_io_body(body: &PyAny, chunk_size: usize) -> PyResult<Bytes> {
    let mut buf = BytesMut::new();
    loop {
        let bytes: &PyBytes = body.call_method1("read", (chunk_size,))?.downcast()?;
        if bytes.as_bytes().is_empty() {
            return Ok(buf.into());
        }
        buf.put(bytes.as_bytes());
    }
}

/// Transform a Twisted ``IRequest`` to an [`http::Request`]
///
/// It uses the following members of ``IRequest``:
///   - ``content``, which is expected to be a file-like object with a ``read`` method
///   - ``uri``, which is expected to be a valid URI as ``bytes``
///   - ``method``, which is expected to be a valid HTTP method as ``bytes``
///   - ``requestHeaders``, which is expected to have a ``getAllRawHeaders`` method
///
/// # Errors
///
/// Returns an error if the Python object doens't properly implement ``IRequest``
pub fn http_request_from_twisted(request: &PyAny) -> PyResult<Request<Bytes>> {
    let content = request.getattr("content")?;
    let body = read_io_body(content, 4096)?;

    let mut req = Request::new(body);

    let uri: &PyBytes = request.getattr("uri")?.downcast()?;
    *req.uri_mut() =
        Uri::try_from(uri.as_bytes()).map_err(|_| PyValueError::new_err("invalid uri"))?;

    let method: &PyBytes = request.getattr("method")?.downcast()?;
    *req.method_mut() = Method::from_bytes(method.as_bytes())
        .map_err(|_| PyValueError::new_err("invalid method"))?;

    {
        let headers = req.headers_mut();
        let headers_iter = request
            .getattr("requestHeaders")?
            .call_method0("getAllRawHeaders")?
            .iter()?;

        for header in headers_iter {
            let header = header?;
            let header: &PyTuple = header.downcast()?;
            let name: &PyBytes = header.get_item(0)?.downcast()?;
            let name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| PyValueError::new_err("invalid header name"))?;

            let values: &PySequence = header.get_item(1)?.downcast()?;
            for index in 0..values.len()? {
                let value: &PyBytes = values.get_item(index)?.downcast()?;
                let value = HeaderValue::from_bytes(value.as_bytes())
                    .map_err(|_| PyValueError::new_err("invalid header value"))?;
                headers.append(name.clone(), value);
            }
        }
    }

    Ok(req)
}

/// Send an [`http::Response`] through a Twisted ``IRequest``
///
/// It uses the following members of ``IRequest``:
///
///  - ``responseHeaders``, which is expected to have a `addRawHeader(bytes, bytes)` method
///  - ``setResponseCode(int)`` method
///  - ``write(bytes)`` method
///  - ``finish()`` method
///
///  # Errors
///
/// Returns an error if the Python object doens't properly implement ``IRequest``
pub fn http_response_to_twisted<B>(request: &PyAny, response: Response<B>) -> PyResult<()>
where
    B: Buf,
{
    let (parts, mut body) = response.into_parts();

    send_parts(request, &parts)?;

    while body.remaining() != 0 {
        let chunk = body.chunk();
        request.call_method1("write", (chunk,))?;
        body.advance(chunk.len());
    }

    request.call_method0("finish")?;

    Ok(())
}

/// Send the [`http::response::Parts`] of a request through Twisted
///
/// # Errors
///
/// Returns an error if the Python object doesn't properly implement ``IRequest``
fn send_parts(request: &PyAny, parts: &http::response::Parts) -> PyResult<()> {
    request.call_method1("setResponseCode", (parts.status.as_u16(),))?;

    let response_headers = request.getattr("responseHeaders")?;
    for (name, value) in parts.headers.iter() {
        response_headers.call_method1("addRawHeader", (name.as_str(), value.as_bytes()))?;
    }

    Ok(())
}

async fn send_body<B>(request: Py<PyAny>, body: B) -> PyResult<()>
where
    B: Body + Send + 'static,
    B::Data: Buf,
    PyErr: From<B::Error>,
{
    futures_util::pin_mut!(body);

    while let Some(res) = body.data().await {
        let mut data = res?;

        Python::with_gil(|py| {
            // Push each chunk
            while data.remaining() != 0 {
                let chunk = data.chunk();
                request.call_method1(py, "write", (chunk,))?;
                data.advance(chunk.len());
            }

            PyResult::Ok(())
        })?;
    }

    Python::with_gil(|py| request.call_method0(py, "finish"))?;

    Ok(())
}

/// Handle a Twisted request through a [`Service`]
///
///  # Errors
///
/// Returns an error if the Python object doens't properly implement ``IRequest``
#[allow(clippy::trait_duplication_in_bounds)]
pub fn handle_twisted_request_through_service<S, E, ResBody>(
    service: S,
    twisted_request: &PyAny,
) -> PyResult<&PyAny>
where
    S: Service<Request<Full<Bytes>>, Response = Response<ResBody>, Error = E> + Send + 'static,
    S::Future: Send,
    ResBody: Body + Send + 'static,
    PyErr: From<E> + From<ResBody::Error>,
{
    let py = twisted_request.py();

    // First, transform the Twisted request to an `http::Request` one
    let request = http_request_from_twisted(twisted_request)?;
    let request = request.map(Full::new);

    // Get an owned version of the object, so we can release the GIL
    let twisted_request = twisted_request.into_py(py);

    // Transform the future to a asyncio future. This releases the GIL
    pyo3_asyncio::tokio::future_into_py(py, async move {
        // Call the service
        let response = Oneshot::new(service, request).await?;
        let (parts, body) = response.into_parts();

        Python::with_gil(|py| send_parts(twisted_request.as_ref(py), &parts))?;

        send_body(twisted_request, body).await
    })
}
