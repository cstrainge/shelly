
fn main()
{
    let now = chrono::Utc::now();

    println!("cargo::rustc-env=SHELLY_BUILD_DATE={}", now.format("%Y-%m-%d"));
    println!("cargo::rustc-env=SHELLY_BUILD_TIME={}", now.format("%H:%M:%S UTC"));
}
