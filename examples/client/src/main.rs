use ctapi_rs::CtClient;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let computer = std::env::var("CITECT_COMPUTER")?;
    let user = std::env::var("CITECT_USER")?;
    let password = std::env::var("CITECT_PASSWORD")?;
    // use ctapi_rs::sys::CtTagValueItems;
    use ctapi_rs::CtTagValueItems;
    let mut value = CtTagValueItems::default();
    let client = CtClient::open(
        Some(&computer),
        Some(&user),
        Some(&password),
        ctapi_rs::CT_OPEN_CRYPT | ctapi_rs::CT_OPEN_READ_ONLY,
    )?;
    let result = client.tag_read_ex("TagExt_DemoTag1", &mut value);
    println!("{result:?} {value:#?}");
    Ok(())
}
