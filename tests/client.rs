use assert_cmd::{assert::Assert, Command};
use assert_fs::NamedTempFile;
use lib::Transaction;
use retry::delay::Fixed;
use serde::de::DeserializeOwned;
use snarkvm::prelude::Output;
use std::fs;

#[test]
fn basic_program() {
    let (_tempfile, account) = new_account();

    // deploy a program, save txid
    let (_program_file, program_path) = load_program("hello");
    let result = Command::cargo_bin("client")
        .unwrap()
        .args(&["program", "deploy", &program_path, "-f", &account])
        .assert()
        .success();
    let transaction: Transaction = parse_output(result);

    // get deployment tx, need to retry until it gets committed
    let result = retry::retry(Fixed::from_millis(1000).take(5), || {
        Command::cargo_bin("client")
            .unwrap()
            .args(&["get", &transaction.id(), "-f", &account])
            .assert()
            .try_success()
    });
    assert!(result.is_ok());

    // execute the program, save txid
    let result = Command::cargo_bin("client")
        .unwrap()
        .args(&[
            "program",
            "execute",
            &program_path,
            "hello",
            "1u32",
            "1u32",
            "-f",
            &account,
        ])
        .assert()
        .success();
    let transaction: Transaction = parse_output(result);

    // get execution tx, assert expected output
    let result = retry::retry(Fixed::from_millis(1000).take(5), || {
        Command::cargo_bin("client")
            .unwrap()
            .args(&["get", &transaction.id().to_string(), "-f", &account])
            .assert()
            .try_success()
    });

    let transaction: Transaction = parse_output(result.unwrap());

    // check the output of the execution is the sum of the inputs
    if let Transaction::Execution { execution, .. } = transaction {
        let transition = execution.peek().unwrap();
        let output = transition.outputs();

        if let Output::Public(_, Some(ref value)) = output[0] {
            assert_eq!("2u32", value.to_string());
        } else {
            panic!("expected a public output");
        }
    } else {
        panic!("expected an execution");
    }
}

#[test]
fn program_validations() {
    let (_tempfile, account) = new_account();
    let (_program_file, program_path) = load_program("hello");

    // fail on execute non deployed command
    Command::cargo_bin("client")
        .unwrap()
        .args(&[
            "program",
            "execute",
            &program_path,
            "hello",
            "1u32",
            "1u32",
            "-f",
            &account,
        ])
        .assert()
        .failure();

    Command::cargo_bin("client")
        .unwrap()
        .args(&["program", "deploy", &program_path, "-f", &account])
        .assert()
        .success();

    // fail on already deployed
    Command::cargo_bin("client")
        .unwrap()
        .args(&["program", "deploy", &program_path, "-f", &account])
        .assert()
        .failure();

    // fail on unknown function
    Command::cargo_bin("client")
        .unwrap()
        .args(&[
            "program",
            "execute",
            &program_path,
            "goodbye",
            "1u32",
            "1u32",
            "-f",
            &account,
        ])
        .assert()
        .failure();

    // fail on missing parameter
    Command::cargo_bin("client")
        .unwrap()
        .args(&[
            "program",
            "execute",
            &program_path,
            "hello",
            "1u32",
            "-f",
            &account,
        ])
        .assert()
        .failure();
}

// HELPERS

/// Generate a tempfile with account credentials and return it along with its path.
/// The file will be removed when it goes out of scope.
fn new_account() -> (NamedTempFile, String) {
    let tempfile = NamedTempFile::new("account.json").unwrap();
    let path = tempfile.path().to_string_lossy().to_string();

    Command::cargo_bin("client")
        .unwrap()
        .args(&["account", "new", "-f", &path])
        .assert()
        .success();

    (tempfile, path)
}

/// Load the source code from the given example file, and return a tempfile along with its path,
/// with the same source code but a randomized name.
/// The file will be removed when it goes out of scope.
fn load_program(program_name: &str) -> (NamedTempFile, String) {
    let program_file = NamedTempFile::new(&program_name).unwrap();
    let path = program_file.path().to_string_lossy().to_string();
    // FIXME hardcoded path
    let source = fs::read_to_string(format!("aleo/{}.aleo", program_name)).unwrap();
    // randomize the name so it's different on every test
    let source = source.replace(
        &format!("{}.aleo", program_name),
        &format!("{}{}.aleo", program_name, unique_id()),
    );
    fs::write(&path, source).unwrap();
    (program_file, path)
}

fn unique_id() -> String {
    uuid::Uuid::new_v4()
        .to_string()
        .split("-")
        .last()
        .unwrap()
        .to_string()
}

/// Extract the command assert output and deserialize it as json
fn parse_output<T: DeserializeOwned>(result: Assert) -> T {
    let output = String::from_utf8(result.get_output().stdout.to_vec()).unwrap();
    serde_json::from_str(&output).unwrap()
}
