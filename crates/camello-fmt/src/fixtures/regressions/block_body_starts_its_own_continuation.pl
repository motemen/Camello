# A line the writer wrapped takes one indent level (INDENT-3), and the scope
# holding it is the bracket the wrapped expression is written in. The body of a
# block written inside one is not part of that expression: the level a wrap in
# its statements took was handed back at the call's closing bracket rather than
# at the block's own brace, and everything between them — the brackets closing
# the call included — came back a level deeper.
$meta->register_hook(fetch => sub {
    return [
        map { $_->basename }
            sort { $a cmp $b } $path->children,
    ];
});
