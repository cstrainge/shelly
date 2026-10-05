# shelly
A Rust based Unix style shell with a typed and structured language syntax.

<p align="center">
  <img src="./Shelly.png" alt="Shelly" width="50%">
</p>

## Variable-prefixed paths

An unquoted path such as `$var/file/file` expands `$var` and keeps the suffix
in the same argument:

```text
let $var = '/tmp'
echo $var/file/file
# Prints: /tmp/file/file
```

This also works in assignments, function arguments, and executable paths such
as `$tools/echo`. Undefined variables report an error rather than expanding to
empty text. Use spaces for arithmetic division, for example `$a / $b`;
`$a/file` is a path, not a division expression.
