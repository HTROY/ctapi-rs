use ctapi_rs::CtClient;
use std::sync::Arc;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let computer = std::env::var("CITECT_COMPUTER")?;
    let user = std::env::var("CITECT_USER")?;
    let password = std::env::var("CITECT_PASSWORD")?;
    let client = Arc::new(CtClient::open(
        Some(&computer),
        Some(&user),
        Some(&password),
        ctapi_rs::CT_OPEN_CRYPT,
    )?);
    let list = client.list_new(0).unwrap();
    list.add_tag("TagExt_DemoTag1").unwrap();
    list.add_tag("TagExt_DemoTag1_Mirror").unwrap();
    list.read().unwrap();
    loop {
        let result = list.read_tag("TagExt_DemoTag1", 0).unwrap();
        println!("{result}");
        let result = list.read_tag("TagExt_DemoTag1_Mirror", 0).unwrap();
        println!("{result}");
        std::thread::sleep(std::time::Duration::from_secs(1));
        list.write_tag("TagExt_DemoTag1", "1").unwrap();
        list.read().unwrap();
    }
}
