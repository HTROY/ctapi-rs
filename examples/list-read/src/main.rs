use ctapi_rs::CtClient;
use std::sync::Arc;

const COMPUTER: &str = "127.0.0.1";
const USER: &str = "Engineer";
const PASSWORD: &str = "Citect";

fn main() {
    let client = Arc::new(CtClient::open(Some(COMPUTER), Some(USER), Some(PASSWORD), 0).unwrap());
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
