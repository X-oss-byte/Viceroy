//! Service types.

use rcgen::generate_simple_self_signed;
use {
    crate::{body::Body, execute::ExecuteCtx, Error},
    axum_server::tls_rustls::RustlsConfig,
    futures::future::{self, Ready},
    hyper::{
        http::{Request, Response},
        server::conn::AddrStream,
        service::Service,
    },
    std::{
        convert::Infallible,
        future::Future,
        net::{IpAddr, SocketAddr},
        pin::Pin,
        task::{self, Poll},
    },
};

/// A Viceroy service uses a Wasm module and a handler function to respond to HTTP requests.
///
/// This service type is used to compile a Wasm [`Module`][mod], and perform the actions necessary
/// to initialize a [`Server`][serv] and bind the service to a local port.
///
/// Each time a connection is received, a [`RequestService`][req-svc] will be created, to
/// instantiate the module and return a [`Response`][resp].
///
/// [mod]: https://docs.rs/wasmtime/latest/wasmtime/struct.Module.html
/// [req-svc]: struct.RequestService.html
/// [resp]: https://docs.rs/http/latest/http/response/struct.Response.html
/// [serv]: https://docs.rs/hyper/latest/hyper/server/struct.Server.html
#[derive(Clone)]
pub struct ViceroyService {
    ctx: ExecuteCtx,
}

impl ViceroyService {
    /// Create a new Viceroy service, using the given handler function and module path.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use std::collections::HashSet;
    /// use viceroy_lib::{Error, ExecuteCtx, ProfilingStrategy, ViceroyService};
    /// # fn f() -> Result<(), Error> {
    /// let ctx = ExecuteCtx::new("path/to/a/file.wasm", ProfilingStrategy::None, HashSet::new())?;
    /// let svc = ViceroyService::new(ctx);
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(ctx: ExecuteCtx) -> Self {
        Self { ctx }
    }

    /// An internal helper, create a [`RequestService`](struct.RequestService.html).
    fn make_service(&self, remote: IpAddr) -> RequestService {
        RequestService::new(self.ctx.clone(), remote)
    }

    /// Bind this service to the given address and start serving responses.
    ///
    /// This will consume the service, using it to start a server that will execute the given module
    /// each time a new request is sent. This function will only return if an error occurs.
    // FIXME KTM 2020-06-22: Once `!` is stabilized, this should be `Result<!, hyper::Error>`.
    pub async fn serve(self, _addr: SocketAddr) -> Result<(), hyper::Error> {
        let http = tokio::spawn(self.clone().http_server());
        let https = tokio::spawn(self.https_server());

        // Ignore errors.
        let _ = tokio::join!(http, https);
        // Ignore errors.
        Ok(())
    }

    async fn http_server(self) {
        let addr = SocketAddr::from(([127, 0, 0, 1], 7878));
        println!("http listening on {}", addr);
        axum_server::bind(addr).serve(self).await.unwrap();
    }

    async fn https_server(self) {
        let subject_alt_names = vec!["localhost".to_string()];

        let cert = generate_simple_self_signed(subject_alt_names).unwrap();

        let config = RustlsConfig::from_pem(
            cert.serialize_pem().unwrap().into_bytes(),
            cert.serialize_private_key_pem().into_bytes(),
        )
        .await
        .unwrap();

        let addr = SocketAddr::new(IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)), 7879);
        println!("https listening on {}", addr);
        axum_server::bind_rustls(addr, config)
            .serve(self)
            .await
            .unwrap();
    }
}

impl<'addr> Service<&'addr AddrStream> for ViceroyService {
    type Response = RequestService;
    type Error = Infallible;
    type Future = Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, addr: &'addr AddrStream) -> Self::Future {
        future::ok(self.make_service(addr.remote_addr().ip()))
    }
}

/// A request service is responsible for handling a single request.
///
/// Most importantly, this structure implements the [`tower::Service`][service] trait, which allows
/// it to be dispatched by [`ViceroyService`][viceroy] to handle a single request.
///
/// This object does not need to be used directly; users most likely should use
/// [`ViceroyService::serve`][serve] to bind a service to a port, or
/// [`ExecuteCtx::handle_request`][handle_request] to generate a response for a request when writing
/// test cases.
///
/// [handle_request]: ../execute/struct.ExecuteCtx.html#method.handle_request
/// [serve]: struct.ViceroyService.html#method.serve
/// [service]: https://docs.rs/tower/latest/tower/trait.Service.html
/// [viceroy]: struct.ViceroyService.html
#[derive(Clone)]
pub struct RequestService {
    ctx: ExecuteCtx,
    remote_addr: IpAddr,
}

impl RequestService {
    /// Create a new request service.
    fn new(ctx: ExecuteCtx, remote_addr: IpAddr) -> Self {
        Self { ctx, remote_addr }
    }
}

impl Service<Request<hyper::Body>> for RequestService {
    type Response = Response<Body>;
    type Error = Error;
    #[allow(clippy::type_complexity)]
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    /// Process the request and return the response asynchronously.
    fn call(&mut self, req: Request<hyper::Body>) -> Self::Future {
        // Request handling currently takes ownership of the context, which is cheaply cloneable.
        let ctx = self.ctx.clone();
        let remote = self.remote_addr;

        // Now, use the execution context to handle the request.
        Box::pin(async move { ctx.handle_request(req, remote).await.map(|result| result.0) })
    }
}
