use anyhow::Result;
use std::path::Path;

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
// test the dirty flags with and without clean shutdown

#[test]
fn repair_keeps_dirty_flags_with_clean_shutdown() -> Result<()> {
    let mut td = TestDir::new()?;
    let input = mk_valid_md(&mut td)?;
    let output = mk_zeroed_md(&mut td)?;
    run_ok(cache_repair_cmd(args!["-i", &input, "-o", &output]))?;
    assert!(get_clean_shutdown(&output)?);

    let dump_in = run_ok_raw(cache_dump_cmd(args![&input]))?;
    let dump_out = run_ok_raw(cache_dump_cmd(args![&output]))?;
    assert_eq!(dump_in.stdout, dump_out.stdout);
    Ok(())
}

// The output is marked as cleanly shut down, so the kernel will trust its
// dirty flags. A crashed input must therefore come out with every mapping
// dirty, or blocks newer than the origin would be dropped on demotion.
fn repair_marks_all_dirty_without_clean_shutdown(input: &Path, td: &mut TestDir) -> Result<()> {
    let (nr_dirty, nr_clean) = count_dirty_mappings(input)?;
    assert!(nr_clean > 0);
    unset_clean_shutdown(input)?;

    let output = mk_zeroed_md(td)?;
    run_ok(cache_repair_cmd(args!["-i", input, "-o", &output]))?;
    run_ok(cache_check_cmd(args![&output]))?;
    assert!(get_clean_shutdown(&output)?);
    assert_eq!(count_dirty_mappings(&output)?, (nr_dirty + nr_clean, 0));
    Ok(())
}

#[test]
fn repair_v1_marks_all_dirty_without_clean_shutdown() -> Result<()> {
    let mut td = TestDir::new()?;
    let input = mk_valid_md_with_version(&mut td, 1)?;
    repair_marks_all_dirty_without_clean_shutdown(&input, &mut td)
}

#[test]
fn repair_v2_marks_all_dirty_without_clean_shutdown() -> Result<()> {
    let mut td = TestDir::new()?;
    let input = mk_valid_md(&mut td)?;
    generate_stale_dirty_bits(&input, 4)?;
    repair_marks_all_dirty_without_clean_shutdown(&input, &mut td)
}

//-----------------------------------------
