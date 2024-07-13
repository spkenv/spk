// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::fs;

use rstest::rstest;

use super::fixtures::*;
use super::Annotation;

// Test - --extra-data field1:value1 --extra-data field2:value2
// Test - --extra-data field1=value1 --extra-data field2=value2
// Test - --extra-data field1:value1 --extra-data field2=value2
// Test - --extra-data field1=value1 --extra-data field2:value2
// Test - --extra-data '{ field1: value1, field2: value2 }'
#[rstest]
#[case(vec!["field1:value1".to_string(), "field2:value2".to_string()])]
#[case(vec!["field1=value1".to_string(), "field2=value2".to_string()])]
#[case(vec!["field1:value1".to_string(), "field2=value2".to_string()])]
#[case(vec!["field1=value1".to_string(), "field2:value2".to_string()])]
#[case(vec!["{field1: value1, field2: value2}".to_string()])]
fn test_cmd_run_create_annotation(#[case] values: Vec<String>) {
    // Setup some data for the key value pairs
    let field1 = "field1".to_string();
    let field2 = "field2".to_string();
    let value1 = "value1".to_string();
    let value2 = "value2".to_string();

    let filenames = Vec::new();

    // Test - --annotation field1:value1 --annotation field2:value2
    let data = Annotation {
        annotation: values,
        annotation_file: filenames,
    };

    let result = data
        .get_data()
        .expect("unable to parse cmd_run --annotation values");
    assert!(!result.is_empty());
    assert!(result.len() == 2);
    println!("r0: {:?}", result[0]);
    assert!(result[0] == (field1, value1));
    println!("r1: {:?}", result[1]);
    assert!(result[1] == (field2, value2));
}

#[rstest]
#[should_panic]
#[case(vec!["field1,value1".to_string(), "field2/value2".to_string()])]
#[should_panic]
#[case(vec!["field1 value1".to_string(), "field2 value2".to_string()])]
#[should_panic]
#[case(vec!["{field1: value1, field2:    - value2    - value1}".to_string()])]
fn test_cmd_run_create_annotation_invalid(#[case] invalid_values: Vec<String>) {
    let filenames = Vec::new();

    // Test - --annotation with_an_invalid_argument_value
    let data = Annotation {
        annotation: invalid_values,
        annotation_file: filenames,
    };

    let _result = data
        .get_data()
        .expect("unable to parse cmd_run --annotation values");
}

#[rstest]
fn test_cmd_run_create_annotation_from_file(tmpdir: tempfile::TempDir) {
    // Setup some data for the key value pairs
    let field1 = "field1".to_string();
    let field2 = "field2".to_string();
    let value1 = "value1".to_string();
    let value2 = "value2".to_string();
    let annotation = format!("{field1}: {value1}\n{field2}: {value2}\n");

    let filename = tmpdir.path().join("filename.yaml");
    fs::write(filename.clone(), annotation)
        .expect("Unable to write annotation to file during setup");
    let filenames = vec![filename];

    let values = Vec::new();

    // Test - --annotation-file filename.yaml
    let data = Annotation {
        annotation: values,
        annotation_file: filenames,
    };

    let result = data
        .get_data()
        .expect("unable to parse cmd_run --annotation values");
    assert!(!result.is_empty());
    assert!(result.len() == 2);
    assert!(result[0] == (field1, value1));
    assert!(result[1] == (field2, value2));
}

#[rstest]
#[should_panic]
fn test_cmd_run_create_annotation_from_file_not_exist(tmpdir: tempfile::TempDir) {
    // Setup a file name that does not exist
    let filename = tmpdir.path().join("nosuchfile.yaml");
    let filenames = vec![filename];

    let values = Vec::new();

    // Test - --annotation-file nosuchfile.yaml
    let data = Annotation {
        annotation: values,
        annotation_file: filenames,
    };

    let _result = data
        .get_data()
        .expect("unable to parse cmd_run --annotation values");
}

#[rstest]
#[should_panic]
fn test_cmd_run_create_annotation_from_file_invalid_keyvalues(tmpdir: tempfile::TempDir) {
    // Setup some data for the key value pairs
    let field1 = "field1".to_string();
    let field2 = "field2".to_string();
    let value1 = "value1".to_string();
    let value2 = "value2".to_string();
    let annotation = format!("{field1}: {value1}\n{field2}:\n    - {value2}\n    - {value1}\n");

    let filename = tmpdir.path().join("filename.yaml");
    fs::write(filename.clone(), annotation)
        .expect("Unable to write annotation to file during setup");
    let filenames = vec![filename];

    let values = Vec::new();

    // Test - --annotation-file filename.yaml that contains more than key-value string pairs
    let data = Annotation {
        annotation: values,
        annotation_file: filenames,
    };

    let _result = data
        .get_data()
        .expect("unable to parse cmd_run --annotation values");
}

#[rstest]
fn test_cmd_run_create_annotation_all(tmpdir: tempfile::TempDir) {
    // Setup some data for the key value pairs
    let field1 = "field1".to_string();
    let field2 = "field2".to_string();
    let field3 = "field3".to_string();
    let field4 = "field4".to_string();

    let value1 = "value1".to_string();
    let value2 = "value2".to_string();
    let value3 = "value3".to_string();
    let value4 = "value4".to_string();

    let values: Vec<String> = vec![
        format!("{field1}:{value1}"),
        format!("{field2}={value2}"),
        "{field3: value3, field4: value4}".to_string(),
    ];

    let annotation = format!("{field4}: {value4}\n{field3}: {value3}\n");

    let filename = tmpdir.path().join("filename.yaml");
    fs::write(filename.clone(), annotation)
        .expect("Unable to write annotation to file during setup");
    let filenames = vec![filename];

    // Test -  --annotation field1:value1 --annotation field2=value2
    //         --annotation '{ field1: value1, field2: value2 }'
    //         --annotation-file filename.yaml
    let data = Annotation {
        annotation: values,
        annotation_file: filenames,
    };

    let result = data
        .get_data()
        .expect("unable to parse cmd_run --annotation values");

    assert!(!result.is_empty());
    assert!(result.len() == 6);
    // from --annotation-file filename.toml
    assert!(result[0] == (field3.clone(), value3.clone()));
    assert!(result[1] == (field4.clone(), value4.clone()));
    // from --annotation field1:value1 annotation field2:value2
    assert!(result[2] == (field1, value1));
    assert!(result[3] == (field2, value2));
    // from --annotation '{field3: value3, field4: value4}'
    assert!(result[4] == (field3, value3));
    assert!(result[5] == (field4, value4));
}
