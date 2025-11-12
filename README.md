# zed-kanata ⌨️

A Zed editor extension that provides syntax highlighting for
[Kanata](https://github.com/jtroo/kanata) keyboard configuration files (`.kbd`).

## ✨ Features

- 🎨 **Syntax highlighting** for `.kbd` files via [tree-sitter-kanata](https://github.com/postsolar/tree-sitter-kanata)
- 🔄 **Auto-indentation** for better code formatting
- 🔗 **Bracket matching** to help with nested configurations
- 💬 **Comment support** with line comments (`;;`)
- 🔍 **Real-time error diagnostics** via custom Rust-based LSP server
  - Automatically downloaded and installed on first use
  - Available for macOS (x86_64, ARM64), Linux (x86_64, ARM64), and Windows (x86_64, ARM64)
  - Powered by the official [kanata-parser](https://github.com/jtroo/kanata)

## 📦 Installation

### From Zed Extensions

Once published to the Zed extension registry:

1. Open Zed
2. Open the command palette (`Cmd+Shift+P` on macOS, `Ctrl+Shift+P` on
   Linux/Windows)
3. Search for "zed: extensions"
4. Search for "kanata" and install

### 🔧 Development Installation

To install the development version:

1. Clone this repository:

```bash
git clone https://github.com/willpuckett/zed-kanata.git
cd zed-kanata
```

2. In Zed:
   - Open the command palette (`Cmd+Shift+P` / `Ctrl+Shift+P`)
   - Run "zed: install dev extension"
   - Select the `zed-kanata` directory

The extension will automatically compile and install. The LSP server binary will be automatically downloaded from GitHub releases on first use.

## 🚀 Usage

Once installed, the extension automatically activates when you open any `.kbd`
file, providing:

- Syntax highlighting for all Kanata configuration syntax
- Auto-indentation for nested expressions
- Bracket matching and navigation

### 📝 Example Configuration

Create a file with the `.kbd` extension:

```kbd
;; My Kanata Configuration
(defcfg
  process-unmapped-keys yes
  danger-enable-cmd no
)

(defsrc
  esc  f1   f2   f3   f4   f5   f6   f7   f8   f9   f10  f11  f12
  grv  1    2    3    4    5    6    7    8    9    0    -    =    bspc
  tab  q    w    e    r    t    y    u    i    o    p    [    ]    \
  caps a    s    d    f    g    h    j    k    l    ;    '    ret
  lsft z    x    c    v    b    n    m    ,    .    /    rsft
  lctl lmet lalt           spc            ralt rmet menu rctl
)

(deflayer default
  _    _    _    _    _    _    _    _    _    _    _    _    _
  _    _    _    _    _    _    _    _    _    _    _    _    _    _
  _    _    _    _    _    _    _    _    _    _    _    _    _    _
  _    _    _    _    _    _    _    _    _    _    _    _    _
  _    _    _    _    _    _    _    _    _    _    _    _
  _    _    _              _              _    _    _    _
)
```

## 🔧 Troubleshooting

### Extension not loading

1. Ensure you have the latest version of Zed
2. Check that the extension is properly installed via the extensions menu
3. Try reinstalling the dev extension

### Syntax highlighting not working

1. Ensure the file has a `.kbd` extension
2. Restart Zed after installation
3. Check Zed's logs (`~/Library/Logs/Zed/Zed.log` on macOS) for errors

## 📄 License

MIT License

## 🔗 Related Projects

- ⌨️ [Kanata](https://github.com/jtroo/kanata) - The keyboard remapper this
  extension supports
- 🌳 [tree-sitter-kanata](https://github.com/postsolar/tree-sitter-kanata) -
  Tree-sitter grammar for Kanata (used by this extension)
- 💻 [vscode-kanata](https://github.com/rszyma/vscode-kanata) - VSCode extension
  for Kanata with language server support

## 🗺️ Roadmap

- [ ] Publish to Zed extension registry
- [x] Language server integration for diagnostics
- [x] Completions and hover information
- [ ] Code snippets for common Kanata patterns
- [ ] Formatting support for aligning deflayer blocks
- [ ] Go-to-definition for aliases and layers

---

**Note**: This is an unofficial extension and is not affiliated with the
official Kanata project.
