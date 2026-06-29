use super::decoder::DecoderCtx;
use super::ferror::FError as FE;
use super::raw::Raw;
use super::stream::{ReadStream, WriteStream};
use super::world::World;
use byteorder::{ByteOrder, LittleEndian};
use std::path::Path;
use std::str;

pub struct Save {
    pub raw: Raw,
    pub worlds: Vec<World>,
}

impl Save {
    const WORLD_TAG: &str = "<world>";
    const CAMPAIGN_TAG: &str = "<campaign>";

    pub fn load(path: &Path) -> Result<Self, FE> {
        let raw = Raw::load_file(path)?;
        let mut worlds = Vec::new();
        
        // 1. Scan the whole file upfront to find the absolute position of EVERY world tag
        let mut world_positions = Vec::new();
        let mut search_cursor = 0;
        
        while let Some(relative_offset) = raw.find_str(Self::WORLD_TAG, search_cursor) {
            let absolute_pos = search_cursor + relative_offset;
            world_positions.push(absolute_pos);
            search_cursor = absolute_pos + Self::WORLD_TAG.len(); 
        }

        if world_positions.is_empty() {
            return Err(FE::NoWorld);
        }

        // 2. Loop through our discovered tag addresses
        for idx in 0..world_positions.len() {
            let world_offset = world_positions[idx];
            
            let world_size = if idx + 1 < world_positions.len() {
                // For chunks 1-11: Distance to the next world tag works perfectly
                world_positions[idx + 1] - world_offset
            } else {
                // For the final chunk: We dynamically extract the exact size from the file's internal sizing header!
                let campaign_offset = match raw.find_str(Self::CAMPAIGN_TAG, world_offset) {
                    Some(campaign) => world_offset + campaign,
                    None => return Err(FE::NoCampaign),
                };

                let mut extracted_size: usize = 0;
                // Scan backward from the campaign tag to extract this chunk's true internal size descriptor
                for j in (campaign_offset - 256..campaign_offset).rev() {
                    let fsize = LittleEndian::read_u32(&raw.mem[j..j + 4]);
                    if fsize & (1 << 31) != 0 {
                        let size = fsize ^ (1 << 31);
                        if size as usize <= campaign_offset - j {
                            extracted_size = j - world_offset;
                            break;
                        }
                    }
                }

                if extracted_size == 0 {
                    // Safe Fallback: If the header calculation glitches, we fallback to remaining file size
                    raw.mem.len() - world_offset
                } else {
                    extracted_size
                }
            };

            // 3. Decode the chunk using perfectly calculated matching dimensions
            let mut rd = ReadStream::new(&raw.mem, world_offset);
            let world = World::decode(&mut rd, (world_offset, world_size))?;
            worlds.push(world);
        }

        Ok(Save { raw, worlds })
    }

    pub fn save(&self, path: &Path) -> Result<(), FE> {
        let mut raws = Vec::new();

        // 1. Gather all starting locations of the world blocks to calculate physical chunk gaps
        let mut world_offsets: Vec<usize> = self.worlds.iter().map(|w| w.offset).collect();
        world_offsets.push(self.raw.mem.len()); // Append total file length as the final boundary line

        // 2. Process every world chunk sequentially
        for idx in 0..self.worlds.len() {
            let world = &self.worlds[idx];
            
            // Calculate the exact true byte space this chunk occupies in the original physical file.
            let full_chunk_span = world_offsets[idx + 1] - world.offset;

            if world.modified {
                // IF MODIFIED: Let the world structural encoder serialize your changes cleanly
                let mut wd = WriteStream::new(0);
                
                // This safely triggers the native World::encode method, including the unparsed trailing flags
                world.encode(&mut wd, ())?; 

                // FIX: Passing 0 as the size parameter triggers the WriteStream's internal auto-sizing.
                // This guarantees the new raw chunk size perfectly reflects the freshly generated DEFLATE payload.
                let new_raw = wd.into_raw(world.offset, 0); 
                
                raws.push(new_raw);
                println!("Packing modified World Chunk [{}] with calculated dimensions...", idx);
            } else {
                // IF UNTOUCHED: Pull the pristine, original byte block directly out of 
                // the source file buffer, keeping all trailing plaintext blocks completely identical!
                let pristine_chunk = Raw {
                    offset: world.offset,
                    size: full_chunk_span,
                    mem: self.raw.mem[world.offset..(world.offset + full_chunk_span)].to_vec(),
                };
                raws.push(pristine_chunk);
            }
        }

        // 3. Re-assemble and seal the multi-chunk container file structure
        self.raw.assemble_file(path, raws)?;
        Ok(())
    }
}
