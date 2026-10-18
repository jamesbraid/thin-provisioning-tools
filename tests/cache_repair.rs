use anyhow::Result;

mod common;

use common::cache::*;
use common::common_args::*;
use common::fixture::*;
use common::input_arg::*;
use common::output_option::*;
use common::program::*;
use common::target::*;
use common::test_dir::*;

//------------------------------------------

const USAGE: &str = "Repair binary cache metadata, and write it to a different device or file

Usage: cache_repair [OPTIONS] --input <FILE> --output <FILE>

Options:
  -h, --help           Print help
  -i, --input <FILE>   Specify the input device
  -o, --output <FILE>  Specify the output device
  -q, --quiet          Suppress output messages, return only exit code.
  -V, --version        Print version";

//-----------------------------------------

struct CacheRepair;

impl<'a> Program<'a> for CacheRepair {
    fn name() -> &'a str {
        "cache_repair"
    }

    fn cmd<I>(args: I) -> Command
    where
        I: IntoIterator,
        I::Item: Into<std::ffi::OsString>,
    {
        cache_repair_cmd(args)
    }

    fn usage() -> &'a str {
        USAGE
    }

    fn arg_type() -> ArgType {
        ArgType::IoOptions
    }

    fn bad_option_hint(option: &str) -> String {
        msg::bad_option_hint(option)
    }
}

impl<'a> InputProgram<'a> for CacheRepair {
    fn mk_valid_input(td: &mut TestDir) -> Result<std::path::PathBuf> {
        mk_valid_md(td)
    }

    fn file_not_found() -> &'a str {
        msg::FILE_NOT_FOUND
    }

    fn missing_input_arg() -> &'a str {
        msg::MISSING_INPUT_ARG
    }

    fn corrupted_input() -> &'a str {
        "bad checksum in superblock"
    }
}

impl<'a> OutputProgram<'a> for CacheRepair {
    fn missing_output_arg() -> &'a str {
        msg::MISSING_OUTPUT_ARG
    }
}

impl<'a> MetadataWriter<'a> for CacheRepair {
    fn file_not_found() -> &'a str {
        msg::FILE_NOT_FOUND
    }
}

//-----------------------------------------

test_accepts_help!(CacheRepair);
test_accepts_version!(CacheRepair);
test_rejects_bad_option!(CacheRepair);

test_input_file_not_found!(CacheRepair);
test_input_cannot_be_a_directory!(CacheRepair);
test_corrupted_input_data!(CacheRepair);

test_missing_output_option!(CacheRepair);

test_readonly_input_file!(CacheRepair);

//-----------------------------------------
// accepts empty argument

#[test]
fn accepts_empty_argument() -> Result<()> {
    let mut td = TestDir::new()?;
    let input = mk_valid_md(&mut td)?;
    let output = mk_zeroed_md(&mut td)?;
    run_ok(cache_repair_cmd(args!["-i", &input, "-o", &output, ""]))?;
    Ok(())
}

//-----------------------------------------
// test the dirty flags without clean shutdown

// The output is marked as cleanly shut down, so the kernel trusts its dirty
// flags. Without a clean shutdown of the input, every mapping must come out
// dirty, or blocks newer than the origin would be lost on demotion.
fn repair_marks_all_dirty_without_clean_shutdown(metadata_version: u8) -> Result<()> {
    let mut td = TestDir::new()?;
    let input = mk_valid_md_with_version(&mut td, metadata_version)?;
    if metadata_version == 2 {
        generate_stale_dirty_bits(&input, 4)?;
    }
    let before = run_ok(cache_dump_cmd(args![&input]))?;
    let nr_mappings = count_mappings(&before, true) + count_mappings(&before, false);
    assert!(count_mappings(&before, false) > 0);
    unset_clean_shutdown(&input)?;

    let output = mk_zeroed_md(&mut td)?;
    run_ok(cache_repair_cmd(args!["-i", &input, "-o", &output]))?;
    run_ok(cache_check_cmd(args![&output]))?;
    assert!(get_clean_shutdown(&output)?);
    let after = run_ok(cache_dump_cmd(args![&output]))?;
    assert_eq!(count_mappings(&after, false), 0);
    assert_eq!(count_mappings(&after, true), nr_mappings);
    Ok(())
}

#[test]
fn repair_v1_marks_all_dirty_without_clean_shutdown() -> Result<()> {
    repair_marks_all_dirty_without_clean_shutdown(1)
}

#[test]
fn repair_v2_marks_all_dirty_without_clean_shutdown() -> Result<()> {
    repair_marks_all_dirty_without_clean_shutdown(2)
}

//-----------------------------------------
