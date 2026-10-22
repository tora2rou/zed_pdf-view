// main.typ から読み込まれる。色や文言を変えて保存してみる。
#let accent-color = rgb("#d62828")

#let greeting(name) = [こんにちは、#name]

#let accent(body) = text(fill: accent-color, weight: "bold", body)
