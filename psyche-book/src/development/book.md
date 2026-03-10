# Building the Psyche Book

That's the document you're reading! :D

## Development

Simply run `just serve_book` to serve the book over http on localhost! This will also automatically rebuild the book when changes are made.

## Building

`nix build .#psyche-book`

The book will be output to `result/`, which you can preview easily with `python -m http.server -d ./result/`
