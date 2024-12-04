fda create xls || true
fda stop xls || true
fda program set xls program.sql --udf-rs udf/src/lib.rs --udf-toml udf/udf.toml
fda restart --recompile xls