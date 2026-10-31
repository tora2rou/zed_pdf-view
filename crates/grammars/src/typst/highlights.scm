; Adapted from uben0/tree-sitter-typst (MIT): queries/typst/highlights.scm.
; Capture names follow Zed's themes. In Zed a later pattern overrides an earlier one for the same
; node, so general patterns come first.

(ident) @variable
(comment) @comment

; CODE
(call
  item: (ident) @function)
(call
  item: (field field: (ident) @function.method))
(tagged field: (ident) @property)
(field field: (ident) @property)

(let "let" @keyword)
(branch ["if" "else"] @keyword)
(while "while" @keyword)
(for ["for" "in"] @keyword)
(import "import" @keyword)
(as "as" @keyword)
(include "include" @keyword)
(show "show" @keyword)
(set "set" @keyword)
(return "return" @keyword)
(flow ["break" "continue"] @keyword)
(context "context" @keyword)
(in ["in" "not"] @keyword)
(and "and" @keyword)
(or "or" @keyword)
(not "not" @keyword)

(sign ["+" "-"] @operator)
(add "+" @operator)
(sub "-" @operator)
(mul "*" @operator)
(div "/" @operator)
(cmp ["==" "<=" ">=" "!=" "<" ">"] @operator)
(fraction "/" @operator)
(fac "!" @operator)
(attach ["^" "_"] @operator)
(wildcard) @operator

(number) @number
(string) @string
(bool) @boolean
(none) @constant
(auto) @constant
(label) @label
(ref) @label

; MARKUP
(heading) @title
(heading ["=" "==" "===" "====" "=====" "======"] @punctuation.markup)
(strong) @emphasis.strong
(emph) @emphasis
(url) @link_uri
(raw_span) @text.literal
(raw_blck) @text.literal
(raw_span "`" @punctuation.markup)
(raw_blck "```" @punctuation.markup)
(raw_blck lang: (ident) @label)
(item "-" @punctuation.list_marker)
(term ["/" ":"] @punctuation.list_marker)
(quote) @punctuation.special
(shorthand) @string.special
(symbol) @string.special
(letter) @variable
(linebreak) @string.escape
(escape) @string.escape
(align) @operator

(math "$" @punctuation.special)
(content ["[" "]"] @punctuation.bracket)
"#" @punctuation.special
"end" @punctuation.special

["(" ")" "{" "}"] @punctuation.bracket
["," ";" ".." ":" "sep"] @punctuation.delimiter
"assign" @punctuation.delimiter
(field "." @punctuation.delimiter)
