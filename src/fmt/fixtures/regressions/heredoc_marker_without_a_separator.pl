# A heredoc marker at the end of a line with neither `,` nor `;` after it: the
# argument list simply continues on the next line, and the body starts there.
f(
    <<'A'
body
A
);
