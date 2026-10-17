use anyhow::{anyhow, Result};
use std::io::Cursor;
use std::path::Path;

use crate::cache::mapping::Mapping;
use crate::cache::superblock::*;
use crate::checksum;
use crate::commands::engine::*;
use crate::devtools::damage_generator::*;
use crate::io_engine::IoEngine;
use crate::pdata::array::{pack_array_block, unpack_array_block};
use crate::pdata::bitset::read_bitset;
use crate::pdata::btree_walker::btree_to_value_vec;
use crate::pdata::space_map::common::*;
use crate::pdata::unpack::unpack;

//------------------------------------------

pub enum DamageOp {
    CreateMetadataLeaks {
        nr_blocks: usize,
        expected_rc: u32,
        actual_rc: u32,
    },
    CreateStaleDirtyBits {
        nr_blocks: usize,
    },
}

pub struct CacheDamageOpts<'a> {
    pub engine_opts: EngineOptions,
    pub op: DamageOp,
    pub output: &'a Path,
}

pub fn damage_metadata(opts: CacheDamageOpts) -> Result<()> {
    let engine = EngineBuilder::new(opts.output, &opts.engine_opts)
        .write(true)
        .build()?;
    let sb = read_superblock(engine.as_ref(), SUPERBLOCK_LOCATION)?;
    let sm_root = unpack::<SMRoot>(&sb.metadata_sm_root)?;

    match opts.op {
        DamageOp::CreateMetadataLeaks {
            nr_blocks,
            expected_rc,
            actual_rc,
        } => create_metadata_leaks(engine, sm_root, nr_blocks, expected_rc, actual_rc),
        DamageOp::CreateStaleDirtyBits { nr_blocks } => {
            create_stale_dirty_bits(engine.as_ref(), &sb, nr_blocks)
        }
    }
}

// Unmap dirty cache blocks but leave their bits in the dirty bitset set.
// This is what the kernel leaves behind if it writes back and demotes a
// block that was dirty at the last suspend, since the v2 dirty bitset is
// only written on suspend.
fn create_stale_dirty_bits(engine: &dyn IoEngine, sb: &Superblock, nr_blocks: usize) -> Result<()> {
    if sb.version < 2 {
        return Err(anyhow!("stale dirty bits require metadata version 2"));
    }

    let dirty_root = sb
        .dirty_root
        .ok_or_else(|| anyhow!("dirty bitset not found"))?;
    let dirty_bits = read_bitset(engine, dirty_root, sb.cache_blocks as usize, false)?;

    let mut path = Vec::new();
    let ablocks = btree_to_value_vec::<u64>(&mut path, engine, false, sb.mapping_root)?;

    let mut nr_unmapped = 0;
    let mut cbegin = 0;
    for blocknr in ablocks {
        if nr_unmapped == nr_blocks {
            break;
        }

        let b = engine.read(blocknr)?;
        let mut ablock = unpack_array_block::<Mapping>(&path, b.get_data())?;
        let mut needs_update = false;

        for (i, m) in ablock.values.iter_mut().enumerate() {
            if nr_unmapped == nr_blocks {
                break;
            }
            if m.is_valid() && dirty_bits.contains(cbegin + i) {
                *m = Mapping::default();
                nr_unmapped += 1;
                needs_update = true;
            }
        }
        cbegin += ablock.header.max_entries as usize;

        if needs_update {
            let mut cursor = Cursor::new(b.get_data());
            pack_array_block(&ablock, &mut cursor)?;
            checksum::write_checksum(b.get_data(), checksum::BT::ARRAY)?;
            engine.write(&b)?;
        }
    }

    if nr_unmapped < nr_blocks {
        return Err(anyhow!(
            "found only {} dirty blocks to unmap, {} requested",
            nr_unmapped,
            nr_blocks
        ));
    }

    Ok(())
}

//------------------------------------------
