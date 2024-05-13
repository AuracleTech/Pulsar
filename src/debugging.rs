#[macro_export]
macro_rules! hyper {
    ($msg:expr) => {
        println!("Hyper: {}", $msg);
    };
}
