use ctapi_rs::CtClient;

const COMPUTER: &str = "192.168.31.198";
const USER: &str = "Engineer";
const PASSWORD: &str = "Citect";

fn main() {
    let mut client = CtClient::open(Some(COMPUTER), Some(USER), Some(PASSWORD), 0).unwrap();
    let mut list = client.list_new(0).unwrap();
    list.add_tag("TagExt_DemoTag1").unwrap();
    list.add_tag("TagExt_DemoTag1_Mirror").unwrap();
    list.read().unwrap();
    loop {
        let result = list.read_tag("TagExt_DemoTag1", 0).unwrap();
        println!("{result}");
        let result = list.read_tag("TagExt_DemoTag1_Mirror", 0).unwrap();
        println!("{result}");
        std::thread::sleep(std::time::Duration::from_secs(1));
        list.write_tag("TagExt_DemoTag1", "1", None).unwrap();
        list.read().unwrap();
    }
}