; comments
(line_comment) @comment.line
(block_comment) @comment.block

; parentheses
[
  "("
  ")"
] @punctuation.bracket

; configuration declarations
(list
  . (unquoted_item) @keyword.directive
    (#match? @keyword.directive "^def(cfg|src)$"))

; layer declarations
(list
  . (unquoted_item) @keyword.function
    (#match? @keyword.function "^def(layer|layermap)$"))

; definition declarations (aliases, variables, etc.)
(list
  . (unquoted_item) @keyword.control
    (#match? @keyword.control "^def(alias|var|fakekeys|virtualkeys|template)$"))

; chord declarations
(list
  . (unquoted_item) @keyword.operator
    (#match? @keyword.operator "^def(chords|chordsv2-experimental)$"))

; conditional and platform-specific declarations
(list
  . (unquoted_item) @keyword.conditional
    (#match? @keyword.conditional "^(platform|defaliasenvcond|def(localkeys-(win|winiov2|wintercept|linux|macos)|overrides|seq))$"))

; named declarations - layers
(list
  .
  ((unquoted_item) @_ (#eq? @_ "deflayer")
    .
    (unquoted_item) @type))

; named declarations - layermaps
(list
  .
  ((unquoted_item) @_ (#eq? @_ "deflayermap")
    .
    (list (unquoted_item) @type)))

; includes
(list
  .
  (unquoted_item) @keyword.control.import (#eq? @keyword.control.import "include")
  .
  [
    (quoted_item)
    (unquoted_item)
  ] @string.special.path)

; platform name
(list
  .
  ((unquoted_item) @_ (#eq? @_ "platform")
    .
    (list (unquoted_item) @type)))

; action functions (layer switching, tap-hold, etc.)
(list
  (list
    .
    (unquoted_item) @function.builtin
      (#match? @function.builtin "^(layer-(switch|while-held|toggle)|tap-hold(-release)?(-keys)?|one-shot|multi|macro|unicode|cmd|caps-word|switch|on-idle-fakekey|fork|tap-dance)$")
    (_)))

; key codes and special keys
((unquoted_item) @constant.builtin
  (#match? @constant.builtin "^(lctl|rctl|lsft|rsft|lalt|ralt|lmet|rmet|caps|ret|esc|tab|bspc|spc|del|ins|home|end|pgup|pgdn|left|right|up|down|kp[0-9]|f[0-9]+|volu|vold|mute|XX|✗|∅|•)$"))

; modifiers in compound keys
((unquoted_item) @operator
  (#match? @operator "^(S|C|A|M|AG|SG)-"))

; other functions
(list
  (list
    .
    (unquoted_item) @function
    (_)))

; strings
(quoted_item) @string

; aliases
((unquoted_item) @string.special.symbol
  (#match? @string.special.symbol "@.+"))

; variables
((unquoted_item) @variable
  (#match? @variable "\\$.+"))

; numbers (for timing values, etc.)
((unquoted_item) @number
  (#match? @number "^[0-9]+$"))

; boolean-like values
((unquoted_item) @constant.builtin.boolean
  (#match? @constant.builtin.boolean "^(yes|no|true|false)$"))
