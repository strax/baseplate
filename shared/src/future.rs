use crate::Result;
use log::warn;
use std::future::Future;

pub async fn retry<F, T>(times: usize, f: fn() -> F) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    let mut counter = 0;
    loop {
        match f().await {
            Err(err) => {
                counter += 1;
                warn!("attempt {}/{} failed", counter, times);
                if counter == times {
                    break Err(err);
                }
            }
            Ok(res) => break Ok(res),
        }
    }
}
