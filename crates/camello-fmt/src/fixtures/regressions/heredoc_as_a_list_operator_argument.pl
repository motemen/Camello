# A heredoc marker as the first argument of a parenthesis-less list operator.
# `matches <<"EOF"` is read as the left shift `matches << "EOF"`, which parses
# the body as code — silently, when the body happens to be valid code.
matches <<"__A__", qr/x/;
a
b
__A__

matches(<<"__B__");
a
__B__
