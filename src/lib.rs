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
use pyo3::{
    exceptions::PyValueError,
    types::{PyBytes, PySequence, PyTuple},
    IntoPy, PyAny, PyErr, PyResult, Python,
};
use tower::{util::Oneshot, Service};

/// Read a file-like Python object by chunks
///
/// # Errors
///
/// Returns an error if calling the ``read`` on the Python object failed
pub fn read_io_body(body: &PyAny, chunk_size: usize) -> PyResult<Bytes> {
    let mut buf = BytesMut::new();
    loop {
        let bytes: &PyBytes = body.call_method1("read", (chunk_size,))?.cast_as()?;
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

    let uri: &PyBytes = request.getattr("uri")?.cast_as()?;
    *req.uri_mut() =
        Uri::try_from(uri.as_bytes()).map_err(|_| PyValueError::new_err("invalid uri"))?;

    let method: &PyBytes = request.getattr("method")?.cast_as()?;
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
            let header: &PyTuple = header.cast_as()?;
            let name: &PyBytes = header.get_item(0)?.cast_as()?;
            let name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| PyValueError::new_err("invalid header name"))?;

            let values: &PySequence = header.get_item(1)?.cast_as()?;
            for index in 0..values.len()? {
                let value: &PyBytes = values.get_item(index)?.cast_as()?;
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

    request.call_method1("setResponseCode", (parts.status.as_u16(),))?;

    let response_headers = request.getattr("responseHeaders")?;
    for (name, value) in parts.headers.iter() {
        response_headers.call_method1("addRawHeader", (name.as_str(), value.as_bytes()))?;
    }

    while body.remaining() != 0 {
        let chunk = body.chunk();
        request.call_method1("write", (chunk,))?;
        body.advance(chunk.len());
    }

    request.call_method0("finish")?;

    Ok(())
}

/// Handle a Twisted request through a [`Service`]
///
///  # Errors
///
/// Returns an error if the Python object doens't properly implement ``IRequest``
pub fn handle_twisted_request_through_service<S, B, E>(
    service: S,
    twisted_request: &PyAny,
) -> PyResult<&PyAny>
where
    S: Service<Request<Bytes>, Response = Response<B>, Error = E> + Send + 'static,
    S::Future: Send,
    B: Buf + Send,
    PyErr: From<E>,
{
    let py = twisted_request.py();

    // First, transform the Twisted request to an `http::Request` one
    let request = http_request_from_twisted(twisted_request)?;

    // Get an owned version of the object, so we can release the GIL
    let twisted_request = twisted_request.into_py(py);

    // Transform the future to a asyncio future. This releases the GIL
    pyo3_asyncio::tokio::future_into_py(py, async move {
        // Call the service
        let response = Oneshot::new(service, request).await?;

        // Now that we ran the service and got our response, re-acquire the GIL so we can reply
        Python::with_gil(|py| {
            // Now we can get back a reference to that object
            let twisted_request = twisted_request.as_ref(py);
            // And actually reply
            http_response_to_twisted(twisted_request, response)
        })
    })
}
