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

const TARGET_EXT: &[&'static str] = &["rv_i", "rv_m"];

fn main() {
    let json_path = PathBuf::from("./data/instr_dict.json");
    let ext_to_name: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        m.insert("rv_i", "RV32I");
        m.insert("rv_m", "RV32M");
        m
    };

    let data = fs::read_to_string(&json_path).expect("Failed to read instr.json");
    let v: Value = serde_json::from_str(&data).expect("Invalid JSON");

    let mut output = String::new();
    output.push_str("define_riscv_isa!(\n");
    output.push_str("Riscv32Instr,\n");

    let mut isa_dict: HashMap<&str, Vec<String>> = HashMap::new();

    for (name, instr) in v.as_object().unwrap() {
        let exts = instr["extension"].as_array().unwrap();

        if let Some(ext) = exts
            .iter()
            .map(|val| val.as_str().unwrap())
            .find(|e| TARGET_EXT.contains(&e))
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

            let format = if fields == ["rd", "rs1", "rs2"] {
                "R"
            } else if fields == ["rd", "rs1", "imm12"] {
                "I"
            } else if fields == ["imm12hi", "rs1", "rs2", "imm12lo"] {
                "S"
            } else if fields == ["bimm12hi", "rs1", "rs2", "bimm12lo"] {
                "B"
            } else if fields == ["rd", "imm20"] {
                "U"
            } else if fields == ["imm"] {
                "J"
            } else if ["ecall", "ebreak", "fence"].contains(&name.as_str()) {
                "I"
            } else if name == "jal" {
                "J"
            } else {
                panic!(
                    "Unknown instruction format for {} with fields: {}",
                    name,
                    fields.join(", ")
                );
            };

            let s = format!(
                "{} {{\n    opcode: {},\n    funct3: {},\n    funct7: {},\n    format: InstrFormat::{},\n}}",
                name.to_uppercase(),
                opcode,
                funct3,
                funct7,
                format
            );

            isa_dict.entry(ext).or_default().push(s);
        }
    }

    for (key, arr) in isa_dict.into_iter() {
        let name = ext_to_name.get(key).unwrap();
        output.push_str(&format!(
            "{}, {}, {{\n",
            name,
            String::from("TABLE_") + *name
        ));
        for instr in arr {
            output.push_str(&format!("{},\n", instr));
        }
        output.push_str("},\n");
    }

    output.push_str(");\n");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("rv32i_gen.rs");
    fs::write(&out_path, output).expect("Failed to write rv32i_gen.rs");

    println!("cargo:rerun-if-changed={}", json_path.display());
}
