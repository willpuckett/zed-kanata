# kanata-lsp

A Language Server Protocol (LSP) server for [Kanata](https://github.com/jtroo/kanata) keyboard configuration files.

## Features

- **Real-time diagnostics**: Parse errors are shown as you type
- **Syntax validation**: Uses the official `kanata-parser` crate to validate configurations

## Installation

### From source

```bash
cargo build --release
sudo cp target/release/kanata-lsp /usr/local/bin/
```

## Usage

The LSP server is automatically used by the [zed-kanata](../) extension when installed.

It can also be used with other editors that support LSP. The server communicates over stdin/stdout.

### Example Neovim configuration

```lua
vim.api.nvim_create_autocmd({"BufEnter", "BufWinEnter"}, {
  pattern = {"*.kbd"},
  callback = function()
    vim.lsp.start({
      name = "kanata-lsp",
      cmd = {"kanata-lsp"},
    })
  end,
})
```

### Example VSCode settings.json

```json
{
  "kanata.lsp.path": "/usr/local/bin/kanata-lsp"
}
```

## Architecture

The server is built with:
- `tower-lsp`: LSP server framework
- `kanata-parser`: Official Kanata configuration parser
- `tokio`: Async runtime

When a `.kbd` file is opened or changed, the server:
1. Writes the content to a temporary file
2. Runs the Kanata parser on it
3. Returns any parse errors as LSP diagnostics

## Development

Run in development mode:

```bash
cargo run
```

The server will wait for LSP client connections on stdin/stdout.

## License

MIT
