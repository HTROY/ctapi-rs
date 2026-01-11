//! Tokio async runtime integration demo
//!
//! This example demonstrates how to use ctapi-rs with Tokio's async/await runtime.
//! 
//! Note: This demo will fail to connect without a running Citect SCADA instance.

use ctapi_rs::{CtClient, TokioCtClient, TokioCtList};
use std::sync::Arc;
use tokio::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Tokio CtAPI Demo ===\n");

    // Connect to Citect SCADA
    let client = match CtClient::open(None, None, None, 0) {
        Ok(c) => {
            println!("✓ Connected to Citect SCADA\n");
            Arc::new(c)
        }
        Err(e) => {
            eprintln!("✗ Failed to connect: {}", e);
            eprintln!("  Make sure Citect SCADA is running");
            return Ok(());
        }
    };

    // Demo 1: Simple async/await calls
    println!("Demo 1: Simple async/await calls");
    println!("-----------------------------------");
    match demo_simple_async(&client).await {
        Ok(_) => println!("✓ Demo 1 completed\n"),
        Err(e) => eprintln!("✗ Demo 1 failed: {}\n", e),
    }

    // Demo 2: Concurrent async operations
    println!("Demo 2: Concurrent async operations");
    println!("-----------------------------------");
    match demo_concurrent_operations(&client).await {
        Ok(_) => println!("✓ Demo 2 completed\n"),
        Err(e) => eprintln!("✗ Demo 2 failed: {}\n", e),
    }

    // Demo 3: Async tag operations
    println!("Demo 3: Async tag operations");
    println!("-----------------------------------");
    match demo_tag_operations(&client).await {
        Ok(_) => println!("✓ Demo 3 completed\n"),
        Err(e) => eprintln!("✗ Demo 3 failed: {}\n", e),
    }

    // Demo 4: Async list operations
    println!("Demo 4: Async list operations");
    println!("-----------------------------------");
    match demo_list_operations(&client).await {
        Ok(_) => println!("✓ Demo 4 completed\n"),
        Err(e) => eprintln!("✗ Demo 4 failed: {}\n", e),
    }

    // Demo 5: Tokio task spawning
    println!("Demo 5: Spawning multiple tokio tasks");
    println!("-----------------------------------");
    match demo_tokio_tasks(&client).await {
        Ok(_) => println!("✓ Demo 5 completed\n"),
        Err(e) => eprintln!("✗ Demo 5 failed: {}\n", e),
    }

    // Demo 6: Timeout handling
    println!("Demo 6: Timeout handling");
    println!("-----------------------------------");
    match demo_timeout_handling(&client).await {
        Ok(_) => println!("✓ Demo 6 completed\n"),
        Err(e) => eprintln!("✗ Demo 6 failed: {}\n", e),
    }

    println!("=== All demos completed ===");
    Ok(())
}

/// Demo 1: Simple async/await calls
async fn demo_simple_async(client: &Arc<CtClient>) -> anyhow::Result<()> {
    // Use async/await syntax for Cicode calls
    let time = client.cicode_tokio("Time(1)", 0, 0).await?;
    println!("Current time: {}", time);

    let date = client.cicode_tokio("Date(4)", 0, 0).await?;
    println!("Current date: {}", date);

    let version = client.cicode_tokio("Version()", 0, 0).await?;
    println!("Citect version: {}", version);

    Ok(())
}

/// Demo 2: Concurrent async operations
async fn demo_concurrent_operations(client: &Arc<CtClient>) -> anyhow::Result<()> {
    let commands = vec![
        ("Time(1)", "time"),
        ("Date(4)", "date"),
        ("Version()", "version"),
        ("Name()", "project name"),
        ("ServerInfo()", "server info"),
    ];

    // Launch all operations concurrently
    let handles: Vec<_> = commands
        .into_iter()
        .map(|(cmd, label)| {
            let client = Arc::clone(client);
            let cmd = cmd.to_string();
            let label = label.to_string();
            tokio::spawn(async move {
                let result = client.cicode_tokio(&cmd, 0, 0).await?;
                Ok::<_, anyhow::Error>((label, result))
            })
        })
        .collect();

    // Wait for all to complete
    for handle in handles {
        match handle.await? {
            Ok((label, result)) => println!("{}: {}", label, result),
            Err(e) => eprintln!("Operation failed: {}", e),
        }
    }

    Ok(())
}

/// Demo 3: Async tag operations
async fn demo_tag_operations(client: &Arc<CtClient>) -> anyhow::Result<()> {
    // Read tags asynchronously
    let tags = vec!["Temperature", "Pressure", "FlowRate"];

    for tag in &tags {
        match client.tag_read_tokio(tag).await {
            Ok(value) => println!("{}: {}", tag, value),
            Err(e) => eprintln!("{}: error - {}", tag, e),
        }
    }

    // Write a tag asynchronously
    match client.tag_write_tokio("Setpoint", "25.5").await {
        Ok(_) => println!("✓ Wrote Setpoint = 25.5"),
        Err(e) => eprintln!("✗ Failed to write: {}", e),
    }

    Ok(())
}

/// Demo 4: Async list operations
async fn demo_list_operations(client: &Arc<CtClient>) -> anyhow::Result<()> {
    let mut list = client.list_new(0)?;

    // Add tags to list
    let tags = vec!["Temperature", "Pressure", "FlowRate"];
    for tag in &tags {
        list.add_tag(tag)?;
    }

    // Read all tags asynchronously
    list.read_tokio().await?;

    // Get values
    for tag in &tags {
        match list.read_tag(tag, 0) {
            Ok(value) => println!("{}: {}", tag, value),
            Err(e) => eprintln!("{}: error - {}", tag, e),
        }
    }

    Ok(())
}

/// Demo 5: Spawning multiple tokio tasks
async fn demo_tokio_tasks(client: &Arc<CtClient>) -> anyhow::Result<()> {
    let mut handles = vec![];

    // Spawn 10 concurrent tasks
    for i in 0..10 {
        let client = Arc::clone(client);
        let handle = tokio::spawn(async move {
            let cmd = format!("StrToInt(\"{}\")", i);
            let result = client.cicode_tokio(&cmd, 0, 0).await?;
            Ok::<_, anyhow::Error>(format!("Task {}: {}", i, result))
        });
        handles.push(handle);
    }

    // Wait for all tasks
    for handle in handles {
        match handle.await? {
            Ok(msg) => println!("{}", msg),
            Err(e) => eprintln!("Task failed: {}", e),
        }
    }

    Ok(())
}

/// Demo 6: Timeout handling
async fn demo_timeout_handling(client: &Arc<CtClient>) -> anyhow::Result<()> {
    use tokio::time::timeout;

    // Set a 2-second timeout
    let timeout_duration = Duration::from_secs(2);

    match timeout(timeout_duration, client.cicode_tokio("Time(1)", 0, 0)).await {
        Ok(Ok(result)) => println!("Result: {}", result),
        Ok(Err(e)) => eprintln!("Operation failed: {}", e),
        Err(_) => eprintln!("Operation timed out after 2 seconds"),
    }

    // Simulate a potentially slow operation
    println!("Testing timeout with potentially slow operation...");
    match timeout(
        Duration::from_millis(100),
        client.cicode_tokio("Sleep(1)", 0, 0),
    )
    .await
    {
        Ok(Ok(result)) => println!("Fast enough: {}", result),
        Ok(Err(e)) => eprintln!("Operation failed: {}", e),
        Err(_) => println!("Operation timed out (as expected)"),
    }

    Ok(())
}
