# data-provider

There's a bunch of functionality here, but the HTTP components are what you probably want to try out first.

## HTTP data provider fetch example

### Usage

#### Working example

First, an example:

`cargo run --example http -- --file-size 40000004052 --batch-ids 103 --token-size 4 --tokenizer tests/resources/llama3_tokenizer.json urls https://storage.googleapis.com/nous-pretraining-public-us/fineweb-1pct-tokenized-llama3/000_fineweb.ds`

This will fetch some FineWeb data and output it using the LLaMA 3 tokenizer.

#### Basic command structure

```bash
cargo run --example http --file-size <SIZE> [--sequence-length <LENGTH>] [--token-size <SIZE>] --batch-ids <IDS> [--tokenizer <PATH>] <SUBCOMMAND>

```

The tool supports two main modes of operation: template-based URLs and explicit URL lists.

#### Required

- `--batch-ids`: Comma-separated list of batch IDs to retrieve

#### Optional

- `--sequence-length`: Length of each sequence (default: 2048)
- `--token-size`: Size of each token in bytes (default: 2)
- `--tokenizer`: Path to a tokenizer file for decoding output

#### Subcommands

##### Template Mode

```bash
template <TEMPLATE> --start <START> --end <END> [--left-pad-zeros <N> (default 0)]
```

Example:

```bash
cargo run --example http --batch-ids 1,2,3 template "http://example.com/{}.ds" --start 0 --end 10
```

This will fetch URLs http://example.com/0.ds through http://example.com/10.ds.

###### Left pad zeros

Using `--left-pad-zeros 3` will transform the fetched URLs to http://example.com/000.ds through http://example.com/010.ds.

##### URL List Mode

```bash
urls <URL1> <URL2> ...
```

Example:

```bash
cargo run --example http --batch-ids 1,2,3 urls "http://example.com/1.ds" "http://example.com/2.ds"
```

### Examples

1. Fetch data using a template with a tokenizer:

```bash
cargo run --example http --batch-ids 1,2,3 --tokenizer ./tokenizer.json template "http://example.com/{}.ds" --start 0 --end 10
```

2. Fetch data using explicit URLs:

```bash
cargo run --example http --sequence-length 1024 --batch-ids 1,2,3 urls "http://example.com/data1.ds" "http://example.com/data2.ds"
```

### Output

The tool will output the retrieved samples for each batch ID. If a tokenizer is specified, the output will be decoded using the tokenizer. Otherwise, the raw sample data will be displayed.
