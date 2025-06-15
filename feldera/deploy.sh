fda create xls || true
fda stop xls || true
fda program set xls program.sql --udf-rs udf/src/lib.rs --udf-toml udf/udf.toml
fda set-config xls workers 8
fda set-config xls storage true
fda set-config xls fault_tolerance at_least_once
fda set-config xls checkpoint_interval 21600
fda restart --recompile xls
