use core::panic;
use serde_json::Value;
use std::{collections::HashMap, env, fs, path::PathBuf};

fn to_bits(s: &str) -> u64 {
    u64::from_str_radix(s, 2).unwrap_or(0)
}

fn get_instr_bits(s: &str, low: usize, high: usize) -> &str {
    let idx1 = s.len() - high - 1;
    let idx2 = s.len() - low - 1;
    &s[idx1..=idx2]
}

fn get_opcode(s: &str) -> u64 {
    to_bits(get_instr_bits(s, 0, 6))
}

fn get_funct3(s: &str) -> u64 {
    to_bits(get_instr_bits(s, 12, 14))
}

fn get_funct7(s: &str) -> u64 {
    to_bits(get_instr_bits(s, 25, 31))
}

fn hex_to_u64(s: &str) -> u64 {
    u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap()
}

fn main() {
    let json_path = PathBuf::from("./data/instr_dict.json");
    let ext_to_name: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("rv_i", "RV32I");
        m.insert("rv_m", "RV32M");
        m.insert("rv64_i", "RV64I");
        m.insert("rv64_m", "RV64M");
        m.insert("rv_zicsr", "RVZicsr");
        m.insert("rv_system", "RVSystem");
        m.insert("rv_f", "RV32F");
        m.insert("rv64_f", "RV64F");
        m.insert("rv_s", "RVS");
        m
    };

    let target_ext = ext_to_name.keys().collect::<Vec<_>>();

    let data = fs::read_to_string(&json_path).expect("Failed to read instr.json");
    let v: Value = serde_json::from_str(&data).expect("Invalid JSON");

    let mut output = String::new();
    output.push_str("define_riscv_isa!(\n");
    output.push_str("RiscvInstr,\n");

    let mut isa_dict: HashMap<&str, Vec<String>> = HashMap::new();

    for (name, instr) in v.as_object().unwrap() {
        let exts = instr["extension"].as_array().unwrap();

        if let Some(ext) = exts
            .iter()
            .map(|val| val.as_str().unwrap())
            .find(|e| target_ext.contains(&e))
        {
            let encoding = instr["encoding"].as_str().unwrap();

            let opcode = get_opcode(encoding);
            let funct3 = get_funct3(encoding);
            let funct7 = get_funct7(encoding);

            let fields = instr["variable_fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| f.as_str().unwrap())
                .collect::<Vec<_>>();

            let format = if fields == ["rd", "rs1", "rs2"]
                || fields == ["rd", "rs1"]
                || fields == ["rs1", "rs2"]
            {
                "R"
            } else if fields == ["rd", "rs1", "rs2", "rm"] || fields == ["rd", "rs1", "rm"] {
                "R_rm"
            } else if fields == ["rd", "rs1", "rs2", "rs3", "rm"] {
                "R4_rm"
            } else if fields == ["rd", "rs1", "imm12"]
                || fields == ["rd", "rs1", "shamtd"]
                || fields == ["rd", "rs1", "shamtw"]
                || fields == ["rd", "csr", "zimm5"]
                || fields == ["rd", "rs1", "csr"]
            {
                "I"
            } else if fields == ["imm12hi", "rs1", "rs2", "imm12lo"] {
                "S"
            } else if fields == ["bimm12hi", "rs1", "rs2", "bimm12lo"] {
                "B"
            } else if fields == ["rd", "imm20"] {
                "U"
            } else if fields == ["imm"] {
                "J"
            } else if ["fence"].contains(&name.as_str()) {
                "I"
            } else if name == "jal" {
                "J"
            } else if fields.is_empty() {
                "None"
            } else {
                panic!(
                    "Unknown instruction format for {} with fields: {}",
                    name,
                    fields.join(", ")
                );
            };

            let mask = hex_to_u64(instr["mask"].as_str().unwrap());
            let key = hex_to_u64(instr["match"].as_str().unwrap());

            let use_mask = fields.contains(&"shamtd")
                || fields.contains(&"shamtw")
                || fields.is_empty()
                || fields.contains(&"rm")
                || ext == "rv_s";

            let s = format!(
                "{} {{\n    opcode: {},\n    funct3: {},\n    funct7: {},\n    format: InstrFormat::{},\n    mask: {},\n    key: {},\n    use_mask: {},\n}}",
                name.to_uppercase(),
                opcode,
                funct3,
                funct7,
                format,
                mask,
                key,
                use_mask
            );

            isa_dict
                .entry(ext_to_name.get(ext).unwrap())
                .or_default()
                .push(s);
        }
    }

    for (name, arr) in isa_dict.into_iter() {
        output.push_str(&format!(
            "{}, {}, {{\n",
            name,
            String::from("TABLE_") + &(*name).to_uppercase()
        ));
        for instr in arr {
            output.push_str(&format!("{},\n", instr));
        }
        output.push_str("},\n");
    }

    output.push_str(");\n");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("rvinstr_gen.rs");
    fs::write(&out_path, output).expect("Failed to write rvinstr_gen.rs");

    println!("cargo:rerun-if-changed={}", json_path.display());
}
