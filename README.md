# Handle Twisted requests through a `tower::Service`

This library helps converting Twisted's [`IRequest`][IRequest] to an `http::Request`, and then sending the `http::Response` back.

[IRequest]: https://docs.twistedmatrix.com/en/latest/api/twisted.web.iweb.IRequest.html

# Usage

## Handle a Twisted request through a Service

```rust
use std::convert::Infallible;

use bytes::Bytes;
use http::{Request, Response};
use pyo3::prelude::*;
use tower::util::BoxCloneService;

use pyo3_twisted_web::handle_twisted_request_through_service;

#[pyclass]
struct Handler {
    service: BoxCloneService<Request<Bytes>, Response<Bytes>, Infallible>,
}

#[pymethods]
impl Handler {
    #[new]
    fn new() -> Self {
        let service = tower::service_fn(|_request: Request<Bytes>| async move {
            let response = Response::new(Bytes::from("hello"));
            Ok(response)
        });

        Self {
            service: BoxCloneService::new(service),
        }
    }

    fn handle<'a>(&self, twisted_request: &'a PyAny) -> PyResult<&'a PyAny> {
        let service = self.service.clone();
        handle_twisted_request_through_service(service, twisted_request)
    }
}

#[pymodule]
fn my_handler(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<Handler>()?;
    Ok(())
}
```

And on the Python side:

```python
from twisted.internet import asyncioreactor
asyncioreactor.install()

import asyncio
from twisted.internet.defer import ensureDeferred, Deferred
from twisted.web.server import NOT_DONE_YET
from twisted.web import server, resource
from twisted.internet import endpoints, reactor

from my_handler import Handler

class MyResource(resource.Resource):
    isLeaf = True

    def __init__(self) -> None:
        super().__init__()
        self.handler = Handler()

    def render(self, request):
        f = self.handler.handle(request)
        ensureDeferred(Deferred.fromFuture(f))
        return NOT_DONE_YET

endpoints.serverFromString(reactor, "tcp:8888").listen(server.Site(MyResource()))
reactor.run()
```

## Define a Twisted `Resource` out of a Service

```rust
use std::convert::Infallible;

use bytes::Bytes;
use http::{Request, Response};
use pyo3::prelude::*;

use pyo3_twisted_web::Resource;

// Via a (sub)class
#[pyclass(extends=Resource)]
struct MyResource;

#[pymethods]
impl MyResource {
    #[new]
    fn new() -> (Self, Resource) {
        let service = tower::service_fn(|_request: Request<Bytes>| async move {
            let response = Response::new(Bytes::from("hello"));
            Ok(response)
        });

        let super_ = Resource::new::<_, _, Infallible>(service);
        (Self, super_)
    }
}

// Via a function
#[pyfunction]
fn get_resource(py: Python) -> PyResult<Py<Resource>> {
    let service = tower::service_fn(|_request: Request<Bytes>| async move {
        let response = Response::new(Bytes::from("hello"));
        Ok(response)
    });

    Py::new(py, Resource::new::<_, _, Infallible>(service))
}

#[pymodule]
fn my_handler(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<MyResource>()?;
    m.add_function(wrap_pyfunction!(get_resource, m)?)?;
    Ok(())
}
```

And on the Python side:

```python
from twisted.internet import asyncioreactor
asyncioreactor.install()

import asyncio
from twisted.web.server import Site
from twisted.internet import endpoints, reactor

from my_handler import MyResource, get_resource

endpoints.serverFromString(reactor, "tcp:8888").listen(Site(MyResource()))
# or
endpoints.serverFromString(reactor, "tcp:8888").listen(Site(get_resource()))

reactor.run()
```

# Limitations

The Twisted [`asyncioreactor`](https://twisted.org/documents/21.2.0/api/twisted.internet.asyncioreactor.html) should be installed. Futures are executed by the [`tokio`](https://tokio.rs/) runtime through [`pyo3-asyncio`](https://github.com/awestlake87/pyo3-asyncio), which requires an asyncio event loop to be running.

The Service must accept an `http::Request<bytes::Bytes>`, and return an `http::Request<impl bytes::Buf>`.
The `Service::Error` must convert to `pyo3::PyErr` (`std::convert::Infallible` is a good candidate if you don't want your handler to throw a Python exception).
