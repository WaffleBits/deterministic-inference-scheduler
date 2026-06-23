use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use rust_inference_runtime::{
    BackendMirrorInput, GateInput, ReplayInput, evaluate_release, mirror_to_gate_input, run_replay,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

fn main() {
    if let Err(error) = run() {
        eprintln!("runtime-lab: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args().skip(1);
    let command = arguments.next().ok_or_else(usage)?;
    let (input, output) = parse_paths(arguments.collect())?;

    match command.as_str() {
        "replay" => {
            let replay_input: ReplayInput = read_json(&input)?;
            write_json(&output, &run_replay(replay_input)?)?;
        }
        "gate" => {
            let gate_input: GateInput = read_json(&input)?;
            write_json(&output, &evaluate_release(&gate_input))?;
        }
        "mirror" => {
            let mirror_input: BackendMirrorInput = read_json(&input)?;
            write_json(&output, &mirror_to_gate_input(&mirror_input)?)?;
        }
        "mirror-gate" => {
            let mirror_input: BackendMirrorInput = read_json(&input)?;
            let gate_input = mirror_to_gate_input(&mirror_input)?;
            write_json(&output, &evaluate_release(&gate_input))?;
        }
        _ => return Err(usage().into()),
    }

    println!("{}", output.display());
    Ok(())
}

fn parse_paths(arguments: Vec<String>) -> Result<(PathBuf, PathBuf), Box<dyn Error>> {
    let mut input = None;
    let mut output = None;
    let mut index = 0;

    while index < arguments.len() {
        match arguments[index].as_str() {
            "--input" => {
                index += 1;
                input = arguments.get(index).map(PathBuf::from);
            }
            "--output" => {
                index += 1;
                output = arguments.get(index).map(PathBuf::from);
            }
            _ => return Err(usage().into()),
        }
        index += 1;
    }

    Ok((input.ok_or_else(usage)?, output.ok_or_else(usage)?))
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, Box<dyn Error>> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut payload = serde_json::to_vec_pretty(value)?;
    payload.push(b'\n');
    fs::write(path, payload)?;
    Ok(())
}

fn usage() -> String {
    "usage: runtime-lab <replay|gate|mirror|mirror-gate> --input <path> --output <path>".into()
}
