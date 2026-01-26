# Data Processing Tools

This set of tools is used to process compressed distro-results, expand them,
sample them, and generate comparison graphs.

## 1. Expand and Chunk Data

Run the `expand-and-chunk.py` script to aggregate JSON data:

```bash
python3 expand-and-chunk.py /path/to/distro_results/ /path/to/output.json
```

## 2. Compare Data

Use the `cmp` Go tool to compare the data using different methods (e.g.,
Jaccard):

```bash
go run ./go-tools/cmd/cmp/main.go -input /path/to/output.json -output ./results/ -method jaccard
```

### Optional Parameters:

- `-P` sets the normalization parameter (e.g., `-P 0.048`).
- `-N` specifies sequence length (e.g., `-N 100`).

## 3. Generate Comparison Graphs

Use `graph-cmp.py` to generate a distribution plot:

```bash
python3 ./python-tools/graph-cmp.py ./results/_jaccard_<params>.csv
```

The plot is saved in the `./results/` directory.
