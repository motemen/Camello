# No blank line straight after an opening brace — but a comment that happens to
# end in `{` opens nothing, and the blank line after it is the writer's.
foo();    # open {

bar();

# {

baz();

sub f {
    1;
}
