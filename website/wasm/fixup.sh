#!/usr/bin/env bash

sed -n '/export class Pubkey {/,/^}/p' pkg/psyche_deserialize_zerocopy_wasm.d.ts > bindings/Pubkey.d.ts

# for each generated binding file
find bindings/ -name "*.ts" -type f | while read -r file; do
    # change import from .ts to .js
    sed -i -E 's/import (.*) from "(\.\/.*)";/import \1 from "\2.js";/g' "$file"
    # and prepend import of pubkey if it needs it
    if grep -q "Pubkey" "$file"; then
        echo 'import { Pubkey } from "./Pubkey.js";' | cat - $file > temp && mv temp $file
    fi


    JSNAME=$(basename $file | sed 's/\.ts/\.js/')
    TYPE=$(basename $file | cut -d "." -f1)

    # and add an export line for that type in the main file
    echo "export { $TYPE } from \"./$JSNAME\"" >> pkg/psyche_deserialize_zerocopy_wasm.d.ts
done

# move bindings into pkg
mv bindings/*.ts ./pkg
