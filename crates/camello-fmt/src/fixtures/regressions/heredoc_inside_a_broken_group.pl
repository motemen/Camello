# A heredoc marker inside a group that breaks over several lines. The body
# belongs to the *physical line* the marker sits on, not to the statement, so
# it has to be written at column 0 straight after that line ends.
foo($input, {
    bar => <<'A',
alpha
beta
A
    baz => '',
});
