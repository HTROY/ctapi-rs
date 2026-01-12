use ctapi_rs::{AsyncCtClient, AsyncOperation, CtClient};

const COMPUTER: &str = "127.0.0.1";
const USER: &str = "Engineer";
const PASSWORD: &str = "Citect";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Async CtAPI Demo ===\n");

    let client = CtClient::open(Some(COMPUTER), Some(USER), Some(PASSWORD), 0)?;
    println!("✓ Connected to Citect SCADA\n");

    // Example 1: Simple async cicode call
    println!("Example 1: Async Cicode Call");
    let mut async_op = AsyncOperation::new();
    client.cicode_async("Time(1)", 0, 0, &mut async_op)?;
    println!("  Started async operation...");

    // Do some other work while waiting
    for i in 1..=3 {
        println!("  Doing other work... {}/3", i);
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Get the result
    let result = async_op.get_result(&client)?;
    println!("  Result: {}\n", result);

    // Example 2: Polling for completion
    println!("Example 2: Polling for Completion");
    async_op.reset();
    client.cicode_async("Date(4)", 0, 0, &mut async_op)?;
    println!("  Started async operation...");

    let mut attempts = 0;
    loop {
        attempts += 1;
        match async_op.try_get_result(&client) {
            Some(Ok(result)) => {
                println!("  Completed after {} checks", attempts);
                println!("  Result: {}\n", result);
                break;
            }
            Some(Err(e)) => {
                eprintln!("  Error: {}\n", e);
                break;
            }
            None => {
                print!(".");
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }

    // Example 3: Multiple concurrent async operations
    println!("Example 3: Multiple Concurrent Operations");
    let mut ops = [
        AsyncOperation::new(),
        AsyncOperation::new(),
        AsyncOperation::new(),
    ];

    let commands = ["Time(1)", "Date(4)", "DspGetEnv(\"Tag\")"];

    for (i, (op, cmd)) in ops.iter_mut().zip(commands.iter()).enumerate() {
        client.cicode_async(cmd, 0, 0, op)?;
        println!("  Started operation {}: {}", i + 1, cmd);
    }

    println!("  Waiting for all operations to complete...");
    for (i, op) in ops.iter_mut().enumerate() {
        match op.get_result(&client) {
            Ok(result) => println!("  Operation {}: {}", i + 1, result),
            Err(e) => eprintln!("  Operation {} error: {}", i + 1, e),
        }
    }
    println!();

    // Example 4: Async list operations
    println!("Example 4: Async List Operations");
    let mut list = client.list_new(0)?;
    list.add_tag("TagExt_DemoTag1")?;
    list.add_tag("TagExt_DemoTag1_Mirror")?;
    println!("  Added tags to list");

    let mut list_async = AsyncOperation::new();
    list.read_async(&mut list_async)?;
    println!("  Started async read...");

    // Wait for completion
    while !list_async.is_complete() {
        print!(".");
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    println!();

    let value1 = list.read_tag("TagExt_DemoTag1", 0)?;
    let value2 = list.read_tag("TagExt_DemoTag1_Mirror", 0)?;
    println!("  Tag1: {}", value1);
    println!("  Tag2: {}\n", value2);

    // Example 5: Cancellation
    println!("Example 5: Operation Cancellation");
    let mut cancel_op = AsyncOperation::new();
    client.cicode_async("PageDisplay(\"Summary\")", 0, 0, &mut cancel_op)?;
    println!("  Started long-running operation...");

    std::thread::sleep(std::time::Duration::from_millis(100));
    // cancel_op.cancel(&client)?;
    // Wait for completion
    while !cancel_op.is_complete() {
        print!(".");
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    println!("  Cancelled operation\n");

    println!("=== Demo Complete ===");
    Ok(())
}
