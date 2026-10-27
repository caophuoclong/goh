# goh.elv — spec-driven completion for elvish through edit:completion:arg-completer.
# Add to rc.elv:  use path/to/goh
# Fail-open: a failing goh yields no candidates and elvish falls back to its own completion.
# Not verified against a real elvish session.

use str

var session = 'elvish-'$pid

# Byte length (the wire uses UTF-8 byte offsets; `count` counts codepoints).
fn bytes {|s| str:trim-space (e:printf '%s' $s | e:wc -c | slurp) }

fn complete {|@words|
  var buffer = (str:join ' ' $words)
  var point = (bytes $buffer)
  var cur-bytes = (bytes $words[-1])
  try {
    e:goh menu --session $session --cwd $pwd --limit 50 --cursor-byte $point --buffer $buffer 2>/dev/null | from-lines | each {|line|
      var f = [(str:split "\x1f" $line)]
      if (and (eq $f[0] (to-string (- $point $cur-bytes))) (eq $f[1] (to-string $point))) {
        edit:complex-candidate (str:trim-right (str:replace '\ ' ' ' $f[5]) ' ') &display=$f[4]
      }
    }
  } catch _ { }
}

fn register {
  e:goh specs 2>/dev/null | from-lines | each {|l|
    var name = (str:split "\t" $l)[0]
    if (not-eq $name '') { set edit:completion:arg-completer[$name] = $complete~ }
  }
}

# Directory history for `cd` suggestions when zoxide is not installed.
set after-chdir = [$@after-chdir {|dir| e:goh visit $dir >/dev/null 2>&1 }]

register
