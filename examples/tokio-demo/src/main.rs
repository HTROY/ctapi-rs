//! Tokio async runtime integration demo
//!
//! This example demonstrates how to use ctapi-rs with Tokio's async/await runtime,
//! including both the `spawn_blocking`-based `TokioCtClient` API and the new
//! OVERLAPPED-based `FutureCtClient` API.
//!
//! Note: This demo will fail to connect without a running Citect SCADA instance.

use ctapi_rs::{CtClient, FutureCtClient, TokioCtClient, TokioCtList};
use std::sync::Arc;
use tokio::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Tokio CtAPI Demo ===\n");

    // Connect to Citect SCADA
    let client = match CtClient::open(Some("127.0.0.1"), Some("Engineer"), Some("Citect"), 0) {
        Ok(c) => {
            println!("✓ Connected to Citect SCADA\n");
            Arc::new(c)
        }
        Err(e) => {
            eprintln!("✗ Failed to connect: {}", e);
            eprintln!("  Make sure Citect SCADA is running and the credentials are correct.");
            return Ok(());
        }
    };

    // ── Demo 1: Simple async/await calls (spawn_blocking) ──────────────────
    println!("Demo 1: Simple async/await calls (spawn_blocking)");
    println!("--------------------------------------------------");
    match demo_simple_async(&client).await {
        Ok(_) => println!("✓ Demo 1 completed\n"),
        Err(e) => eprintln!("✗ Demo 1 failed: {}\n", e),
    }

    // ── Demo 2: OVERLAPPED-native Future (no spawn_blocking) ───────────────
    println!("Demo 2: OVERLAPPED-native Future (FutureCtClient)");
    println!("--------------------------------------------------");
    match demo_future_client(&client).await {
        Ok(_) => println!("✓ Demo 2 completed\n"),
        Err(e) => eprintln!("✗ Demo 2 failed: {}\n", e),
    }

    // ── Demo 3: Concurrent async operations ────────────────────────────────
    println!("Demo 3: Concurrent async operations");
    println!("------------------------------------");
    match demo_concurrent_operations(&client).await {
        Ok(_) => println!("✓ Demo 3 completed\n"),
        Err(e) => eprintln!("✗ Demo 3 failed: {}\n", e),
    }

    // ── Demo 4: Async tag operations ───────────────────────────────────────
    println!("Demo 4: Async tag read / write");
    println!("------------------------------");
    match demo_tag_operations(&client).await {
        Ok(_) => println!("✓ Demo 4 completed\n"),
        Err(e) => eprintln!("✗ Demo 4 failed: {}\n", e),
    }

    // ── Demo 5: Extended tag read (value + quality/timestamp) ──────────────
    println!("Demo 5: Extended tag read (metadata)");
    println!("-------------------------------------");
    match demo_tag_read_ex(&client).await {
        Ok(_) => println!("✓ Demo 5 completed\n"),
        Err(e) => eprintln!("✗ Demo 5 failed: {}\n", e),
    }

    // ── Demo 6: Async list operations ──────────────────────────────────────
    println!("Demo 6: Async list operations");
    println!("------------------------------");
    match demo_list_operations(&client).await {
        Ok(_) => println!("✓ Demo 6 completed\n"),
        Err(e) => eprintln!("✗ Demo 6 failed: {}\n", e),
    }

    // ── Demo 7: Spawning multiple Tokio tasks ──────────────────────────────
    println!("Demo 7: Spawning multiple Tokio tasks");
    println!("--------------------------------------");
    match demo_tokio_tasks(&client).await {
        Ok(_) => println!("✓ Demo 7 completed\n"),
        Err(e) => eprintln!("✗ Demo 7 failed: {}\n", e),
    }

    // ── Demo 8: Timeout handling ───────────────────────────────────────────
    println!("Demo 8: Timeout handling");
    println!("------------------------");
    match demo_timeout_handling(&client).await {
        Ok(_) => println!("✓ Demo 8 completed\n"),
        Err(e) => eprintln!("✗ Demo 8 failed: {}\n", e),
    }

    // ── Demo 9: Mix FutureCtClient with try_join! ──────────────────────────
    println!("Demo 9: Concurrent OVERLAPPED futures via try_join!");
    println!("----------------------------------------------------");
    match demo_concurrent_futures(&client).await {
        Ok(_) => println!("✓ Demo 9 completed\n"),
        Err(e) => eprintln!("✗ Demo 9 failed: {}\n", e),
    }

    println!("=== All demos completed ===");
    Ok(())
}

// ── Demo implementations ────────────────────────────────────────────────────

/// Demo 1: Simple async/await calls using the spawn_blocking approach.
async fn demo_simple_async(client: &Arc<CtClient>) -> anyhow::Result<()> {
    let time = client.cicode_tokio("Time(1)", 0, 0).await?;
    println!("  Current time    : {}", time);

    let date = client.cicode_tokio("Date(4)", 0, 0).await?;
    println!("  Current date    : {}", date);

    let version = client.cicode_tokio("Version()", 0, 0).await?;
    println!("  Citect version  : {}", version);

    Ok(())
}

/// Demo 2: The `FutureCtClient` trait drives Windows OVERLAPPED I/O directly,
/// so no blocking thread is consumed per call.
async fn demo_future_client(client: &Arc<CtClient>) -> anyhow::Result<()> {
    // cicode_future() returns a CtApiFuture that implements std::future::Future.
    // It can be .await-ed just like any other future.
    let time = client.cicode_future("Time(1)", 0, 0)?.await?;
    println!("  Time (OVERLAPPED) : {}", time);

    let date = client.cicode_future("Date(4)", 0, 0)?.await?;
    println!("  Date (OVERLAPPED) : {}", date);

    Ok(())
}

/// Demo 3: Launch several async operations concurrently using Tokio tasks.
async fn demo_concurrent_operations(client: &Arc<CtClient>) -> anyhow::Result<()> {
    let commands = vec![
        ("Time(1)", "time"),
        ("Date(4)", "date"),
        ("Version()", "version"),
        ("Name()", "project name"),
    ];

    let handles: Vec<_> = commands
        .into_iter()
        .map(|(cmd, label)| {
            let c = Arc::clone(client);
            let cmd = cmd.to_string();
            let label = label.to_string();
            tokio::spawn(async move {
                let result = c.cicode_tokio(&cmd, 0, 0).await?;
                Ok::<_, anyhow::Error>((label, result))
            })
        })
        .collect();

    for handle in handles {
        match handle.await? {
            Ok((label, result)) => println!("  {:<15}: {}", label, result),
            Err(e) => eprintln!("  operation failed: {}", e),
        }
    }

    Ok(())
}

/// Demo 4: Async tag read and write.
async fn demo_tag_operations(client: &Arc<CtClient>) -> anyhow::Result<()> {
    let tags = vec!["BIT_1", "BIT_2", "BIT_3"];

    for tag in &tags {
        match client.tag_read_tokio(tag).await {
            Ok(value) => println!("  read  {} = {}", tag, value),
            Err(e) => eprintln!("  read  {} → error: {}", tag, e),
        }
    }

    // Write using tag_write_tokio (accepts any string value)
    match client.tag_write_tokio("BIT_1", "1").await {
        Ok(_) => println!("  write BIT_1 = 1  ✓"),
        Err(e) => eprintln!("  write BIT_1 → error: {}", e),
    }

    match client.tag_write_tokio("BIT_1", "0").await {
        Ok(_) => println!("  write BIT_1 = 0  ✓"),
        Err(e) => eprintln!("  write BIT_1 → error: {}", e),
    }

    Ok(())
}

/// Demo 5: Extended tag read — returns value *and* quality/timestamp metadata.
async fn demo_tag_read_ex(client: &Arc<CtClient>) -> anyhow::Result<()> {
    match client.tag_read_ex_tokio("BIT_1").await {
        Ok((value, meta)) => {
            // CtTagValueItems fields are in a packed struct; copy before use.
            let ts = { meta.timestamp };
            let quality = { meta.quality_general };
            println!(
                "  BIT_1 = {}  |  timestamp = {}  |  quality = {}",
                value, ts, quality
            );
        }
        Err(e) => eprintln!("  tag_read_ex BIT_1 → error: {}", e),
    }
    Ok(())
}

/// Demo 6: Async list operations with `TokioCtList`.
async fn demo_list_operations(client: &Arc<CtClient>) -> anyhow::Result<()> {
    let mut list = client.list_new(0)?;

    let tags = vec!["BIT_1", "BIT_2", "BIT_3"];
    for tag in &tags {
        list.add_tag(tag)?;
    }
    println!("  Added {} tags to list", tags.len());

    // Non-blocking read — polls OVERLAPPED status with tokio::time::sleep.
    list.read_tokio().await?;
    println!("  Read complete:");

    for tag in &tags {
        match list.read_tag(tag, 0) {
            Ok(value) => println!("    {} = {}", tag, value),
            Err(e) => eprintln!("    {} → error: {}", tag, e),
        }
    }

    // Async write for a single tag
    list.write_tag_tokio("BIT_1", "1").await?;
    println!("  Wrote BIT_1 = 1  ✓");

    Ok(())
}

/// Demo 7: Spawn 10 concurrent Tokio tasks, each executing a Cicode call.
async fn demo_tokio_tasks(client: &Arc<CtClient>) -> anyhow::Result<()> {
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let c = Arc::clone(client);
            tokio::spawn(async move {
                let cmd = format!("StrToInt(\"{}\")", i);
                let result = c.cicode_tokio(&cmd, 0, 0).await?;
                Ok::<_, anyhow::Error>(format!("  task {:2}: {}", i, result))
            })
        })
        .collect();

    for handle in handles {
        match handle.await? {
            Ok(msg) => println!("{}", msg),
            Err(e) => eprintln!("  task failed: {}", e),
        }
    }

    Ok(())
}

/// Demo 8: Apply a timeout to an async operation.
async fn demo_timeout_handling(client: &Arc<CtClient>) -> anyhow::Result<()> {
    use tokio::time::timeout;

    // Fast operation — should complete well within 2 s.
    match timeout(Duration::from_secs(2), client.cicode_tokio("Time(1)", 0, 0)).await {
        Ok(Ok(result)) => println!("  Time (within 2 s): {}", result),
        Ok(Err(e)) => eprintln!("  operation failed: {}", e),
        Err(_) => eprintln!("  timed out after 2 s"),
    }

    // Same pattern with FutureCtClient — OVERLAPPED future also supports timeout.
    match timeout(
        Duration::from_secs(2),
        client.cicode_future("Date(4)", 0, 0)?,
    )
    .await
    {
        Ok(Ok(result)) => println!("  Date  (within 2 s): {}", result),
        Ok(Err(e)) => eprintln!("  operation failed: {}", e),
        Err(_) => eprintln!("  timed out after 2 s"),
    }

    Ok(())
}

/// Demo 9: Use `tokio::try_join!` with OVERLAPPED futures to run two Cicode
/// calls truly concurrently without any blocking threads.
async fn demo_concurrent_futures(client: &Arc<CtClient>) -> anyhow::Result<()> {
    // cicode_future() returns a CtApiFuture (std::future::Future).
    // try_join! polls both futures concurrently on the same thread.
    let (time, date, version) = tokio::try_join!(
        client.cicode_future("Time(1)", 0, 0)?,
        client.cicode_future("Date(4)", 0, 0)?,
        client.cicode_future("Version()", 0, 0)?,
    )?;

    println!("  time    = {}", time);
    println!("  date    = {}", date);
    println!("  version = {}", version);

    Ok(())
}
