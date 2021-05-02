#![feature(map_first_last)]

use std::collections::BTreeSet;
use std::collections::{BTreeMap, HashMap};
use std::env::args;
use std::fs::File;
use std::io::prelude::*;
use std::io::BufWriter;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use flate2::read::GzDecoder;
use separator::Separatable;
use serde::Deserialize;

use anyhow::Result;

// This is from the CSV file
#[derive(Debug, Deserialize, Clone)]
struct Record {
    place_osmtype: char,
    place_id: u64,
    place_name: String,
    place_type: String,
    place_lat: f64,
    place_lon: f64,
    boundary_osmtype: char,
    boundary_id: u64,
    boundary_name: String,
    boundary_admin_level: String,
}

impl PartialOrd for Record {
    fn partial_cmp(&self, other: &Record) -> Option<std::cmp::Ordering> {
        Some(
            self.place_id
                .cmp(&other.place_id)
                .then(self.boundary_id.cmp(&other.boundary_id)),
        )
    }
}
impl Ord for Record {
    fn cmp(&self, other: &Record) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl PartialEq for Record {
    fn eq(&self, other: &Record) -> bool {
        self.place_id == other.place_id && self.boundary_id == other.boundary_id
    }
}
impl Eq for Record {}

impl std::hash::Hash for Record {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.place_osmtype.hash(state);
        self.place_id.hash(state);
        self.place_name.hash(state);
        self.place_type.hash(state);

        self.boundary_osmtype.hash(state);
        self.boundary_id.hash(state);
        self.boundary_name.hash(state);
        self.boundary_admin_level.hash(state);
    }
}

// OSM URL of an object
fn url(t: char, id: u64) -> String {
    format!(
        "https://www.openstreetmap.org/{}/{}",
        match t {
            'n' => "node",
            'w' => "way",
            'r' => "relation",
            _ => unreachable!(),
        },
        id
    )
}

impl Record {
    fn place_url(&self) -> String {
        url(self.place_osmtype, self.place_id)
    }

    fn boundary_url(&self) -> String {
        url(self.boundary_osmtype, self.boundary_id)
    }
}

impl std::fmt::Display for Record {
    fn fmt(&self, w: &mut std::fmt::Formatter) -> std::result::Result<(), std::fmt::Error> {
        write!(w, "There is a `place={p_tag}` called [{p_name} (node {p_id_sep})]({p_url}) in [{b_name} (rel. {b_id_sep})]({b_url}) (`admin_level={b_level}`)",
            p_name=self.place_name, p_url=self.place_url(), p_tag=self.place_type,
            p_id_sep=self.place_id.separated_string(),
            b_name=self.boundary_name, b_url=self.boundary_url(), b_level=self.boundary_admin_level,
            b_id_sep=self.boundary_id.separated_string(),
        )
    }
}

fn place_dist(r1: &Record, r2: &Record) -> isize {
    haversine_dist(r1.place_lat, r1.place_lon, r2.place_lat, r2.place_lon).round() as isize
}

fn haversine_dist(mut th1: f64, mut ph1: f64, mut th2: f64, ph2: f64) -> f64 {
    ph1 -= ph2;
    ph1 = ph1.to_radians();
    th1 = th1.to_radians();
    th2 = th2.to_radians();
    let dz: f64 = th1.sin() - th2.sin();
    let dx: f64 = ph1.cos() * th1.cos() - th2.cos();
    let dy: f64 = ph1.sin() * th1.cos();
    static EARTH_RADIUS_M: f64 = 6_371_000.;
    ((dx * dx + dy * dy + dz * dz).sqrt() / 2.0).asin() * 2.0 * EARTH_RADIUS_M
}

fn main() -> Result<()> {

    println!("{} version {} Affero GPL source code: {}",
             option_env!("CARGO_PKG_NAME").unwrap_or("NAME NOT SET"),
             option_env!("CARGO_PKG_VERSION").unwrap_or("VERSION NOT SET"),
             option_env!("CARGO_PKG_REPOSITY").unwrap_or("SOURCE CODE REPO NOT SET"),
            );

    let input_filename = args().nth(1).expect("arg 1 should be csv filename");
    let output_filename = args().nth(2).expect("arg 2 should be csv output filename");

    // For each place_id, these records for that
    let mut points_in_boundary: HashMap<u64, Vec<Record>> = HashMap::new();

    println!("Reading in {}", input_filename);
    let input_file = GzDecoder::new(File::open(&input_filename)?);

    let mut rdr = csv::Reader::from_reader(input_file);
    let mut num_records = 0;

    let mut unknown_place_tags: HashMap<String, usize> = HashMap::new();

    for result in rdr.deserialize() {
        let record: Record = result?;

        // where name is set to empty string
        // Initially this wasn't done, so lots of the later tweaks to reduce memory usage might be
        // removed.
        if record.place_name.is_empty() || record.boundary_name.is_empty() {
            continue;
        }
        match record.place_type.as_str() {
            // Use these `place` values
            "city" | "town" | "village" | "suburb" | "neighbourhood" | "square" | "quarter"
            | "islet" | "island" | "municipality" | "city_block" | "district" | "BAMYANGA"
            | "borough" | "block" | "hamlet" => {
                points_in_boundary
                    .entry(record.place_id)
                    .or_default()
                    .push(record);
                num_records += 1;
            }
            // ignore these `place` values
            "locality" | "isolated_dwelling" | "farm" | "country" | "unknown" | "plot" | "yes"
            | "field" | "county" | "state" | "single_dwelling" | "region" | "fixme" | "FIXME"
            | "allotments" => {
                continue;
            }
            x => {
                *unknown_place_tags.entry(x.to_string()).or_default() += 1;
            }
        }
    }
    let num_unknown: usize = unknown_place_tags.values().sum();
    let top_unknown = unknown_place_tags
        .iter()
        .fold(Vec::new(), |mut totals, el| {
            totals.push((el.1, el.0));
            totals.sort_by_key(|x| -(*x.0 as isize));
            totals.truncate(5);
            totals
        });
    println!(
        "There are {} name/contain pairs ({} unknown place tags {}% of total. Top unknowns: {})",
        num_records.separated_string(),
        num_unknown.separated_string(),
        (num_unknown * 100) / num_records,
        top_unknown
            .iter()
            .map(|(count, tag)| format!(
                "{} {} ({}%)",
                tag,
                count.separated_string(),
                *count * 100 / num_unknown
            ))
            .collect::<Vec<String>>()
            .join(", "),
    );

    // Often, in OSM, there is a `place` node for each admin boundary.
    // e.g. Paris is node 17807753 name=Paris,place=coty
    // We want to remove that,
    // that's against the spirit of what we're looking for.
    println!("Removing places which are inside a boundary with the same name");
    points_in_boundary
        .retain(|_point_id, records| !records.iter().any(|r| r.place_name == r.boundary_name));

    let total_records = points_in_boundary
        .values()
        .fold(0, |acc, recs| acc + recs.len());
    println!(
        "Have removed {} ({:.1}%) places",
        (num_records - total_records).separated_string(),
        ((num_records - total_records) as f32 / num_records as f32) * 100.
    );

    println!("Generating name lookup");
    let place_names = points_in_boundary
        .values()
        .flat_map(|recs| recs.iter())
        .fold(
            HashMap::with_capacity(num_records) as HashMap<&str, Vec<&Record>>,
            |mut map, rec| {
                map.entry(&rec.place_name).or_default().push(rec);
                map
            },
        );

    // A chain, is what we are building. It's a list of records.

    // Working list
    // first is the negative of the chain length (neg → longest sorted first)
    // 2nd is the sum of the geographic distance of each step. This prioritizes chains that jump /
    // zigzag over the world, which is more interesting
    // 3rd is the actual chain itself.
    let mut intermediate_chains: BTreeSet<(isize, isize, Vec<&Record>)> = BTreeSet::new();

    // Finished chains go here, indexed by their first record. We only need one chain for each
    // "start" point. We keep the longest chain.
    // This is to reduce memory usage, and maybe could be removed.
    let mut finished_chains: HashMap<&Record, Vec<&Record>> = HashMap::new();
    let mut num_steps_done = 0;

    // The initial chains are all the "point X is in boundary Y", i.e. 1 element chains
    for rec in points_in_boundary.values().flat_map(|recs| recs.iter()) {
        if place_names.contains_key(rec.boundary_name.as_str()) {
            intermediate_chains.insert((-1, 0, vec![rec]));
        }
    }

    let len_initial_intermediate_chains = intermediate_chains.len();

    let mut last_boundary_name;

    let mut longest_seen = -1;

    let max_intermediate = 8_000_000;

    let ctrlc_pressed = Arc::new(AtomicBool::new(false));
    let r = ctrlc_pressed.clone();
    ctrlc::set_handler(move || {
        r.store(true, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    // The main loop that does the calculation.
    // Take the longest intermediate chain we have, and see if we can extend it.
    println!("Starting main loop calculation. Press Ctrl-C to stop going further");
    loop {
        let (neg_chain_len, chain_place_dist, chain) = match intermediate_chains.pop_first() {
            // No more intermediate chains, so we're finished
            None => {
                break;
            }
            Some(x) => x,
        };
        longest_seen = std::cmp::min(longest_seen, neg_chain_len);

        if ctrlc_pressed.load(Ordering::SeqCst) {
            // User has pressed Ctrl C
            println!("Ctrl-C pressed, breaking out of calculation with what we have now");
            break;
        }

        last_boundary_name = &chain.last().unwrap().boundary_name;
        match place_names.get(last_boundary_name.as_str()) {
            None => {
                // can't go any further
                // Keep this chain if it is longer than the longest chain (by number of steps)
                // we've seen for this start point.
                if finished_chains
                    .get(chain[0])
                    .map_or(true, |curr| chain.len() > curr.len())
                {
                    finished_chains.insert(chain[0], chain);
                }
            }

            Some(records) => {
                for rec in records {
                    // ensure the place_id isn't in the chain already.
                    if !chain
                        .iter()
                        .any(|r| r.place_id == rec.place_id || r.boundary_id == rec.boundary_id)
                    {
                        // create a new chain, and add that to the intermediate chains
                        let mut new_chain = chain.clone();
                        new_chain.push(rec);
                        intermediate_chains.insert((
                            -(new_chain.len() as isize),
                            chain_place_dist
                                - place_dist(
                                    new_chain[new_chain.len() - 2],
                                    new_chain[new_chain.len() - 1],
                                ),
                            new_chain,
                        ));
                    } else {
                        // this would be a loop, so stop here and add this chain
                        // again, only if it's longer
                        if finished_chains
                            .get(chain[0])
                            .map_or(true, |curr| chain.len() > curr.len())
                        {
                            finished_chains.insert(chain[0], chain.clone());
                        }
                    }
                }
            }
        }

        // memory management. stop the intermediate_chains from getting too big
        while intermediate_chains.len() > max_intermediate {
            println!("Doing memory clean up");

            // save what we have if we have an intermediate chain that's longer than a finished
            // chain we've seen.
            for (_, _, chain) in intermediate_chains.iter() {
                if chain.len() > 1
                    && finished_chains
                        .get(chain[0])
                        .map_or(true, |curr| chain.len() > curr.len())
                {
                    finished_chains.insert(chain[0], chain.clone());
                }
            }

            // Keep chains of len 1, which are the initial building blocks
            // and any chain which is at least as long as the longest for this start minus 10.
            // i.e. throw away any intermediate chains which are much shorter than the longest for
            // this start point
            intermediate_chains.retain(|(_, _, chain)| {
                chain.len() == 1
                    || finished_chains.get(chain[0]).map_or(true, |longest_seen| {
                        chain.len() >= longest_seen.len().saturating_sub(10)
                    })
            });
            dbg!(intermediate_chains.len());

            // failsafe, just delete the lowest ones
            while intermediate_chains.len() > max_intermediate {
                intermediate_chains.pop_last();
            }
            dbg!(intermediate_chains.len());
        }

        // Print progress report
        num_steps_done += 1;
        if num_steps_done % 10_000 == 0 {
            println!(
                "Done {} steps, intermediate_chains: {} finished_chains: {} longest: {}",
                num_steps_done.separated_string(),
                (intermediate_chains.len() - len_initial_intermediate_chains).separated_string(),
                finished_chains.len().separated_string(),
                -longest_seen
            );
        }

        // Don't go forever
        if num_steps_done >= 1e12 as usize {
            break;
        }
    }

    // Update the finished chains
    for (_, _, chain) in intermediate_chains.into_iter() {
        if chain.len() == 1 {
            continue;
        }
        if let Some(old_chain) = finished_chains.get(chain[0]) {
            if old_chain.len() < chain.len() {
                finished_chains.insert(chain[0], chain);
            }
        } else {
            finished_chains.insert(chain[0], chain);
        }
    }


    let totals_per_len = finished_chains.iter().fold(
        BTreeMap::new() as BTreeMap<usize, usize>,
        |mut tot, (_, chain)| {
            *tot.entry(chain.len()).or_default() += 1;
            tot
        },
    );
    for (len, total) in totals_per_len {
        println!("{:>6}: {:>10}", len, total.separated_string());
    }

    let mut output_file = BufWriter::new(File::create(&output_filename)?);

    let total_finished_chains = finished_chains.len();
    println!(
        "Have {} chains. Writing to {}",
        total_finished_chains.separated_string(),
        output_filename
    );
    let mut num_written_out = 0;

    // Print out chains (except the 1 element chains)
    let mut chains = finished_chains
        .into_iter()
        .filter_map(|(_start_rec, chain)| if chain.len() > 1 { Some(chain) } else { None })
        .collect::<Vec<_>>();
    dbg!(chains.len());
    chains.sort_by_key(|ch| -(ch.len() as isize));

    for chain in chains {
        writeln!(&mut output_file, "chain of len {}:", chain.len())?;
        for (i, r) in chain.iter().enumerate() {
            writeln!(&mut output_file, "{}: {}\n", i, r)?;
        }
        writeln!(&mut output_file)?;
        num_written_out += 1;

        if num_written_out > 1000 {
            break;
        }
    }

    println!(
        "Wrote out {} of {} ({:.1}%)",
        num_written_out.separated_string(),
        total_finished_chains.separated_string(),
        (num_written_out as f64 / total_finished_chains as f64) * 100.
    );
    println!("Finished");
    Ok(())
}
