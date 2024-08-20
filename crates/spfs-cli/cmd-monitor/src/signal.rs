use std::pin::Pin;

use futures::future::Future;
use spfs::Error;

pub trait SignalHandler {
    fn build_signal_future() -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send>>;
}

#[cfg(unix)]
pub mod unix_signal_handler {
    use tokio::signal::unix::{signal, SignalKind};

    use super::*;

    pub struct UnixSignalHandler;

    impl SignalHandler for UnixSignalHandler {
        fn build_signal_future() -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send>> {
            Box::pin(async move {
                let mut interrupt = signal(SignalKind::interrupt())
                    .map_err(|err| Error::process_spawn_error("signal()", err, None))?;
                let mut quit = signal(SignalKind::quit())
                    .map_err(|err| Error::process_spawn_error("signal()", err, None))?;
                let mut terminate = signal(SignalKind::terminate())
                    .map_err(|err| Error::process_spawn_error("signal()", err, None))?;

                futures::future::select_all(vec![
                    Box::pin(interrupt.recv()),
                    Box::pin(quit.recv()),
                    Box::pin(terminate.recv()),
                ])
                .await;

                Ok(())
            })
        }
    }
}

#[cfg(windows)]
pub mod windows_signal_handler {
    use tokio::signal::ctrl_c;

    use super::*;

    pub struct WindowsSignalHandler;

    impl SignalHandler for WindowsSignalHandler {
        fn build_signal_future() -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send>> {
            Box::pin(async move {
                let mut interrupt =
                    ctrl_c().map_err(|err| Error::process_spawn_error("ctrl_c()", err, None))?;
                let mut quit =
                    ctrl_c().map_err(|err| Error::process_spawn_error("ctrl_c()", err, None))?;
                let mut terminate =
                    ctrl_c().map_err(|err| Error::process_spawn_error("ctrl_c()", err, None))?;

                futures::future::select_all(vec![
                    Box::pin(interrupt),
                    Box::pin(quit),
                    Box::pin(terminate),
                ])
                .await;

                Ok(())
            })
        }
    }
}
