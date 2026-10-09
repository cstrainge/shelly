
use chrono::Utc;

fn main()
{
    let now = Utc::now();

    println!("cargo::rustc-env=SHELLY_BUILD_DATE={}", now.format("%Y-%m-%d"));
    println!("cargo::rustc-env=SHELLY_BUILD_TIME={}", now.format("%H:%M:%S UTC"));
}
