use ctapi_rs::CtClient;

const COMPUTER: &str = "127.0.0.1";
const USER: &str = "Manager";
const PASSWORD: &str = "Citect";

fn main() {
    // use ctapi_rs::sys::CtTagValueItems;
    use ctapi_rs::CtTagValueItems;
    let mut value = CtTagValueItems::default();
    let client = CtClient::open(Some(COMPUTER), Some(USER), Some(PASSWORD), 0).unwrap();
    let result = client.tag_read_ex("TagExt_DemoTag1", &mut value);
    println!("{result:?} {value:#?}");
}
