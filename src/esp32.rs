use std::process::ExitStatus;
use std::time::Duration;

use crate::prelude::*;
use ej_builder_sdk::BuilderSdk;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio_serial::SerialPortBuilderExt;
use tracing::warn;

use crate::{board_folder, results_path};

async fn run_idf_command(sdk: &BuilderSdk, command: &str) -> Result<ExitStatus> {
    let board_path = board_folder(&sdk.config_path(), sdk.board_name());
    Ok(Command::new("bash")
        .arg("-c")
        .arg(&format!(
            ". /media/pi/pi_external/esp/esp-idf/export.sh && idf.py -C {} {}",
            board_path.display(),
            command
        ))
        .spawn()?
        .wait()
        .await?)
}

pub async fn build_esp32s3(sdk: &BuilderSdk) -> Result<()> {
    let result = run_idf_command(sdk, "build").await?;

    if !result.success() {
        warn!(
            "Build failed for ESP32. This happens when new source files are added. Performing a clean build"
        );
        // https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-guides/tools/idf-py.html#select-the-target-chip-set-target
        // https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-guides/tools/idf-py.html#reconfigure-the-project-reconfigure
        // `set-target` performs a clean build and reconfigures the project which is important in
        // case files were added or removed from the source tree
        let result = run_idf_command(sdk, &format!("set-target {}", sdk.board_name())).await?;
        assert!(result.success(), "Clean Failed");
        let result = run_idf_command(sdk, "build").await?;
        assert!(result.success(), "Build Failed");
    }

    Ok(())
}

pub async fn run_esp32s3(sdk: &BuilderSdk) -> Result<()> {
    let results_p = results_path(&sdk.config_path(), &sdk.board_config_name());
    let _ = std::fs::remove_file(&results_p);

    let result = run_idf_command(sdk, "flash").await?;

    assert!(result.success());

    // TODO: Create some udev rules to avoid having to hardcode this
    // Fine for now but will need to be done when new boards are added
    let port = tokio_serial::new("/dev/ttyACM0", 115_200)
        .timeout(Duration::from_secs(120))
        .open_native_async()?;

    let mut reader = BufReader::new(port);

    let mut output = String::new();
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line).await?;

        if n == 0 {
            return Err(Error::TimeoutWaitingForBenchmarkToEnd(output));
        }

        output.push_str(&line[..n]);

        if output.contains("Benchmark Over") {
            std::fs::write(results_p, output)?;
            return Ok(());
        }
    }
}
