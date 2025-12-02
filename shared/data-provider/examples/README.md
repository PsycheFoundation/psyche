# HTTP Data Provider Examples

This directory contains examples for using the HTTP data provider functionality.

## Examples

```bash
# Using numbered files template
cargo run --example http -- --batch-ids 0,1,2 template "https://example.com/{}.ds" --start 0 --end 10

# Using explicit URL list
cargo run --example http -- --batch-ids 0,1,2 urls "https://example.com/1.ds" "https://example.com/2.ds"

# Using GCP bucket
cargo run --example http -- --batch-ids 0,1,2 gcp bucket-name "my-bucket"

# WeightedHTTP with config
cargo run --example http -- --batch-ids 0,1,2 weighted-config shared/data-provider/examples/sample-weighted-config.json
```
