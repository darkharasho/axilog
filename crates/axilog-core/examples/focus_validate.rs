//! Validate `analysis::focus` against real logs: does the shipped pass
//! reproduce the 300-log study's commander separation?
use axilog_core::analysis::focus;
use axilog_core::evtc::decode_raw;

fn med(v: &mut Vec<f64>) -> f64 {
    if v.is_empty() { return f64::NAN }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (mut com_idx, mut other_idx) = (Vec::new(), Vec::new());
    let (mut com_downs, mut oth_downs) = (0u64, 0u64);
    let (mut com_pd, mut oth_pd) = (0.0f64, 0.0f64);

    let mut logs = 0u32;
    for path in &args {
        let Ok(bytes) = std::fs::read(path) else { continue };
        let Ok(raw) = decode_raw(&bytes) else { continue };
        let enc = axilog_core::model::resolve(&raw);
        if enc.kind != "wvw" { continue }
        let d = focus::build(&enc, &raw);
        if d.squad_size < 5 || d.total_casts == 0 { continue }
        if !enc.players.iter().any(|p| p.in_squad && p.commander) { continue }
        logs += 1;
        for (i, p) in enc.players.iter().enumerate() {
            if !p.in_squad { continue }
            let f = d.at(i);
            
            if p.commander {
                com_idx.push(f.focus_index);
                com_downs += f.downs; com_pd += f.pre_down_casts as f64;
            } else {
                other_idx.push(f.focus_index);
                oth_downs += f.downs; oth_pd += f.pre_down_casts as f64;
            }
        }
    }
    println!("logs={logs}");
    println!("commander focus_index  median {:.2}x  (n={})", med(&mut com_idx), com_idx.len());
    println!("other    focus_index  median {:.2}x  (n={})", med(&mut other_idx), other_idx.len());
    println!("pre-down casts/down    com {:.2}  others {:.2}  lift {:.2}x",
        com_pd / com_downs.max(1) as f64, oth_pd / oth_downs.max(1) as f64,
        (com_pd / com_downs.max(1) as f64) / (oth_pd / oth_downs.max(1) as f64));
    println!("downs: com={com_downs} others={oth_downs}");
}
