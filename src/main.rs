#![deny(rust_2018_idioms)]
use clap::{Parser, Subcommand, ValueEnum};
use std::collections::HashMap;
use std::io::{stdout, BufWriter, Write};
use std::path::Path;

mod fot;
use fot::attributes::Attributes;
use fot::esh::{ESH, ESHValue};
use fot::entity::Entity;
use fot::entitylist::EntityList;
use fot::save::Save;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Input file path (.sav or .ent)
    #[arg(short, long)]
    input: String,

    // Specify save file or ent file type
    //#[arg(value_enum)]
    //kind: Kind,

    /// Output file path
    #[arg(short, long)]
    output: String,

    /// Target a specific world chunk index (0 to 11). If omitted, defaults to all or 0.
    #[arg(long)]
    world: Option<usize>, // <-- ADD THIS PARAMETER HERE

    /// Selected entities ids
    #[arg(long)]
    ids: Option<String>,

    /// key=value pairs to find entities (i.e. key1=value1,key2=value2 will return entities with one of the matching pairs)
    #[arg(long)]
    find: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum Kind {
    Save,
    Ent,
}

#[derive(Subcommand, Debug)]
enum Commands {
    ListEntities,
    /// Find entities, kv = key1=value,key2=value2
    FindEntities,
    /// List ESH values of selected entities
    ListValues,
    /// Write ESH value to entity
    WriteValue {
        name: String,
        value: String
    },
    /// Read nested ESH value from entity's ESH
    ReadNested {
        nested: String
    },
    /// Write ESH "value" on "name" into nested ESH at "nested" in entity's ESH
    WriteNested {
        nested: String,
        name: String,
        value: String
    },
    /// List entity attributes (like special stats and skills)
    ListAttributes,
    /// List entity modifiers (buffs/debuffs for attributes)
    ListModifiers,
    /// Write attribute value where group is stats/traits/derived/skills/skill_tags/opt_traits/perks/addictions
    WriteAttribute {
        group: String,
        name: String,
        value: String,
    },
    /// Write modifier value where group is stats/traits/derived/skills/skill_tags/opt_traits/perks/addictions
    WriteModifier {
        group: String,
        name: String,
        value: String,
    },
}

fn log_entities<'a>(entlist: &EntityList, iter: impl IntoIterator<Item = (usize, &'a Entity)>) {
    let mut bf = BufWriter::new(stdout().lock());
    for (id, ent) in iter {
        let type_name = if ent.type_idx != 0xFFFF {
            entlist.get_type_name(ent.type_idx).str.as_str()
        } else {
            "<no type>"
        };
        write!(bf, "{}\t{}\n", id, type_name).expect("failed to write stdout");
    }
}

fn parse_kv(kv: &String) -> Vec<(&str, &str)> {
    kv.split(",")
        .map(|kv| kv.split_once("="))
        .collect::<Option<Vec<(&str, &str)>>>()
        .unwrap()
}

fn from_ids(entlist: &EntityList, line: String) -> HashMap<usize, &Entity> {
    line.split(",")
        .map(|id| {
            (
                id.parse::<usize>().expect("parse id"),
                entlist.get_entity(id.parse().expect("parse id")),
            )
        })
        .collect::<HashMap<usize, &Entity>>()
}

fn from_ids_mut(entlist: &mut EntityList, line: String) -> HashMap<usize, &mut Entity> {
    let mut entities: HashMap<usize, &mut Entity> = HashMap::new();
    let ids: Vec<usize> = line
        .split(",")
        .map(|id| id.parse().expect("id parse"))
        .collect();
    
    for (id, ent) in entlist {
        if ids.contains(&id) {
            entities.insert(id, ent);
        }
    }

    entities
}

fn find_entities(entlist: &EntityList, line: String) -> HashMap<usize, &Entity> {
    let kv = parse_kv(&line);
    let mut entities: HashMap<usize, &Entity> = HashMap::new();
    for (id, ent) in entlist {
        let esh = match &ent.esh {
            Some(esh) => esh,
            None => continue,
        };

        for (name, value) in &esh.props {
            let key = name.str.as_str();
            let svalue = value.to_string();
            for (k, v) in &kv {
                if key == *k && svalue == *v {
                    entities.insert(id, ent);
                }
            }
        }
    }

    entities
}

fn find_entities_mut(entlist: &mut EntityList, line: String) -> HashMap<usize, &mut Entity> {
    let kv = parse_kv(&line);
    let mut entities: HashMap<usize, &mut Entity> = HashMap::new();
    for (id, ent) in entlist {
        let esh = match &ent.esh {
            Some(esh) => esh,
            None => continue,
        };

        'check: for (name, value) in &esh.props {
            let key = name.str.as_str();
            let svalue = value.to_string();
            for (k, v) in &kv {
                if key == *k && svalue == *v {
                    entities.insert(id, ent);
                    break 'check;
                }
            }
        }
    }

    entities
}

fn get_entities(
    entlist: &EntityList,
    ids: Option<String>,
    find: Option<String>,
) -> HashMap<usize, &Entity> {
    if let Some(ids) = ids {
        from_ids(entlist, ids)
    } else if let Some(find) = find {
        find_entities(entlist, find)
    } else {
        panic!("No entity selector provided!")
    }
}

fn get_entities_mut(
    entlist: &mut EntityList,
    ids: Option<String>,
    find: Option<String>,
) -> HashMap<usize, &mut Entity> {
    if let Some(ids) = ids {
        from_ids_mut(entlist, ids)
    } else if let Some(find) = find {
        find_entities_mut(entlist, find)
    } else {
        panic!("No entity selector provided!")
    }
}

fn log_esh(esh: &ESH) {
    let mut bf = BufWriter::new(stdout().lock());
    for (name, value) in &esh.props {
        write!(bf, "{}\t{}\n", name, value).expect("stdout");
    }
    write!(bf, "\n").expect("stdout");
}

fn list_values(ent: &Entity) {
    let esh = match ent.esh.as_ref() {
        Some(esh) => esh,
        None => return
    };

    log_esh(esh);
}

fn write_esh(esh: &mut ESH, name: &String, value: &String) {
    use ESHValue as EV;
    match esh.props.get_mut(name.as_str()).unwrap() {
        EV::Bool(val) => *val = value.parse().expect("parse"),
        EV::Float(val) => *val = value.parse().expect("parse"),
        EV::Int(val) => *val = value.parse().expect("parse"),
        EV::String(val) => val.str = value.clone(),
        EV::Sprite(val) => val.str = value.clone(),
        EV::Enum(val) => val.str = value.clone(),
        EV::EntityFlags(val) => val.entity_id = value.parse().expect("parse"),
        _ => panic!("unsupported ESH type input")
    }
}

fn write_value(ent: &mut Entity, name: &String, value: &String) {
    let esh = ent.get_esh_mut().expect("failed to get esh");
    write_esh(esh, name, value);
}

fn read_nested(ent: &Entity, nested: &String) {
    let esh = ent.get_esh().expect("failed to get esh");
    let nested_esh = esh.get_nested(nested.as_str()).expect("failed to get nested");
    log_esh(&nested_esh);
}

fn write_nested(ent: &mut Entity, nested: &String, name: &String, value: &String) {
    let esh = ent.get_esh_mut().expect("failed to get esh");
    let mut nested_esh = esh.get_nested(nested.as_str()).expect("failed to get nested");
    write_esh(&mut nested_esh, name, value);
    esh.set_nested(nested.as_str(), nested_esh).expect("failed to set nested esh");
}

fn log_attributes(attrs: Attributes) {
    let mut bf = BufWriter::new(stdout().lock());

    write!(bf, "stats\n").expect("stdout");
    for (name, value) in attrs.stats {
        write!(bf, "\t{}\t{}\n", name, value).expect("stdout");
    }
    write!(bf, "traits\n").expect("stdout");
    for (name, value) in attrs.traits {
        write!(bf, "\t{}\t{}\n", name, value).expect("stdout");
    }
    write!(bf, "derived\n").expect("stdout");
    for (name, value) in attrs.derived {
        write!(bf, "\t{}\t{}\n", name, value).expect("stdout");
    }
    write!(bf, "skills\n").expect("stdout");
    for (name, value) in attrs.skills {
        write!(bf, "\t{}\t{}\n", name, value).expect("stdout");
    }
    write!(bf, "skill_tags\n").expect("stdout");
    for (name, value) in attrs.skill_tags {
        write!(bf, "\t{}\t{}\n", name, value).expect("stdout");
    }
    write!(bf, "opt_traits\n").expect("stdout");
    for (name, value) in attrs.opt_traits {
        write!(bf, "\t{}\t{}\n", name, value).expect("stdout");
    }
    write!(bf, "perks\n").expect("stdout");
    for (name, value) in attrs.perks {
        write!(bf, "\t{}\t{}\n", name, value).expect("stdout");
    }
    write!(bf, "addictions\n").expect("stdout");
    for (name, value) in attrs.addictions {
        write!(bf, "\t{}\t{}\n", name, value).expect("stdout");
    }
}

fn list_attributes(ent: &Entity) {
    match ent.get_attributes() {
        Ok(attrs) => log_attributes(attrs),
        Err(e) => panic!("Fatal Error {}", e),
    }
}

fn list_modifiers(ent: &Entity) {
    match ent.get_modifiers() {
        Ok(attrs) => log_attributes(attrs),
        Err(e) => panic!("Fatal Error {}", e),
    }
}

fn write_attribute_value(attrs: &mut Attributes, group: &str, name: &str, value: &str) {
    match group {
        "stats" => attrs.stats[name] = value.parse().expect("parse"),
        "traits" => attrs.traits[name] = value.parse().expect("parse"),
        "derived" => attrs.derived[name] = value.parse().expect("parse"),
        "skills" => attrs.skills[name] = value.parse().expect("parse"),
        "skill_tags" => attrs.skill_tags[name] = value.parse().expect("parse"),
        "opt_traits" => attrs.opt_traits[name] = value.parse().expect("parse"),
        "perks" => attrs.perks[name] = value.parse().expect("parse"),
        "addictions" => attrs.addictions[name] = value.parse().expect("parse"),
        _ => panic!("invalid group specified"),
    }
}

fn write_attribute(ent: &mut Entity, group: &str, name: &str, value: &str) {
    let mut attrs = ent.get_attributes().expect("get_attributes");
    write_attribute_value(&mut attrs, group, name, value);
    ent.set_attributes(attrs).expect("set_attributes");
}

fn write_modifier(ent: &mut Entity, group: &str, name: &str, value: &str) {
    let mut attrs = ent.get_modifiers().expect("modifiers");
    write_attribute_value(&mut attrs, group, name, value);
    ent.set_modifiers(attrs).expect("set_modifiers");
}

fn do_save(cli: Cli) {
    let mut save = match Save::load(Path::new(cli.input.as_str())) {
        Ok(save) => save,
        Err(fe) => panic!("{}", fe),
    };

    match cli.command {
        Commands::ListEntities => {
            // Check if the user specified a single world chunk
            if let Some(target_idx) = cli.world {
                if let Some(world) = save.worlds.get(target_idx) {
                    println!("--- LISTING ENTITIES IN WORLD CHUNK [{}] ---", target_idx);
                    log_entities(&world.entlist, (&world.entlist).into_iter());
                } else {
                    println!("Error: World index {} out of bounds.", target_idx);
                }
            } else {
                // Default fallback: Print everything, but with a structural header label
                for (idx, world) in save.worlds.iter().enumerate() {
                    println!("--- WORLD CHUNK [{}] ENTITIES ---", idx);
                    log_entities(&world.entlist, (&world.entlist).into_iter());
                }
            }
        }

        Commands::FindEntities => {
            let search_term = cli.find.clone().unwrap();
            if let Some(target_idx) = cli.world {
                if let Some(world) = save.worlds.get(target_idx) {
                    log_entities(&world.entlist, find_entities(&world.entlist, search_term));
                }
            } else {
                for (idx, world) in save.worlds.iter().enumerate() {
                    let results = find_entities(&world.entlist, search_term.clone());
                    // Only print headers if this specific map actually contains matching items
                    if !results.is_empty() { 
                        println!("--- FOUND MATCHES IN WORLD CHUNK [{}] ---", idx);
                        log_entities(&world.entlist, results);
                    }
                }
            }
        }

        Commands::ListValues => {
            let target_world_idx = cli.world.unwrap_or(0);

            if let Some(world) = save.worlds.get(target_world_idx) {
                println!("--- VALUES IN WORLD CHUNK [{}] ---", target_world_idx);
                for (_, ent) in get_entities(&world.entlist, cli.ids.clone(), cli.find.clone()) {
                    list_values(ent);
                }
            } else {
                println!("Error: World index {} out of bounds.", target_world_idx);
            }
        }
        
        Commands::WriteValue { name, value } => {
            let target_world_idx = cli.world.unwrap_or(0);

            if let Some(world) = save.worlds.get_mut(target_world_idx) {
                for (_, ent) in get_entities_mut(&mut world.entlist, cli.ids.clone(), cli.find.clone()) {
                    write_value(ent, &name, &value);
                }
                println!("Successfully wrote value to World Chunk [{}]", target_world_idx);

                world.modified = true;
            } else {
                panic!("Target write world index {} does not exist.", target_world_idx);
            }
            
            save.save(Path::new(&cli.output)).expect("failed to save");
        }

        Commands::ReadNested { nested } => {
            let target_world_idx = cli.world.unwrap_or(0);

            if let Some(world) = save.worlds.get(target_world_idx) {
                println!("--- NESTED DATA IN WORLD CHUNK [{}] ---", target_world_idx);
                for (_, ent) in get_entities(&world.entlist, cli.ids.clone(), cli.find.clone()) {
                    read_nested(ent, &nested);
                }
            } else {
                println!("Error: World index {} out of bounds.", target_world_idx);
            }
        }

        Commands::WriteNested { nested, name, value } => {
            let target_world_idx = cli.world.unwrap_or(0);

            if let Some(world) = save.worlds.get_mut(target_world_idx) {
                for (_, ent) in get_entities_mut(&mut world.entlist, cli.ids.clone(), cli.find.clone()) {
                    write_nested(ent, &nested, &name, &value);
                }
                println!("Successfully wrote nested parameters to World Chunk [{}]", target_world_idx);

                world.modified = true;
            } else {
                panic!("Target write world index {} does not exist.", target_world_idx);
            }
            
            save.save(Path::new(&cli.output)).expect("failed to save");
        }

        Commands::ListAttributes => {
            let target_world_idx = cli.world.unwrap_or(0);

            if let Some(world) = save.worlds.get(target_world_idx) {
                println!("--- ATTRIBUTES IN WORLD CHUNK [{}] ---", target_world_idx);
                for (_, ent) in get_entities(&world.entlist, cli.ids.clone(), cli.find.clone()) {
                    list_attributes(ent);
                }
            } else {
                println!("Error: World index {} out of bounds.", target_world_idx);
            }
        }

        Commands::ListModifiers => {
            let target_world_idx = cli.world.unwrap_or(0);

            if let Some(world) = save.worlds.get(target_world_idx) {
                println!("--- MODIFIERS IN WORLD CHUNK [{}] ---", target_world_idx);
                for (_, ent) in get_entities(&world.entlist, cli.ids.clone(), cli.find.clone()) {
                    list_modifiers(ent);
                }
            } else {
                println!("Error: World index {} out of bounds.", target_world_idx);
            }
        }

        Commands::WriteAttribute { group, name, value } => {
            let target_world_idx = cli.world.unwrap_or(0);

            if let Some(world) = save.worlds.get_mut(target_world_idx) {
                for (_, ent) in get_entities_mut(&mut world.entlist, cli.ids.clone(), cli.find.clone()) {
                    write_attribute(ent, group.as_str(), name.as_str(), value.as_str());
                }
                println!("Successfully wrote attribute to World Chunk [{}]", target_world_idx);

                world.modified = true;
            } else {
                panic!("Target write world index {} does not exist.", target_world_idx);
            }
            
            save.save(Path::new(&cli.output)).expect("failed to save");
        }

        Commands::WriteModifier { group, name, value } => {
            // Determine our exact write target index (Defaulting strictly to World 0)
            let target_world_idx = cli.world.unwrap_or(0);

            if let Some(world) = save.worlds.get_mut(target_world_idx) {
                for (_, ent) in get_entities_mut(&mut world.entlist, cli.ids.clone(), cli.find.clone()) {
                    write_modifier(ent, group.as_str(), name.as_str(), value.as_str());
                }
                println!("Successfully wrote modifier to World Chunk [{}]", target_world_idx);

                world.modified = true;
            } else {
                panic!("Target write world index {} does not exist.", target_world_idx);
            }
            
            // Save the entire multi-chunk architecture back safely
            save.save(Path::new(&cli.output)).expect("failed to save");
        }
    }
}

fn main() {
    let cli = Cli::parse();

    do_save(cli);
}