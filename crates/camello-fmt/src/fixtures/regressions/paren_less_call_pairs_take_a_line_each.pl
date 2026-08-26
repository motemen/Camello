# The brackets break, so each element of the list they hold takes a line
# (INDENT-2). The pairs after `f Str` reached that rule only once they stopped
# being the argument list of a call with no parentheses.
my $foo = (
    bar => f Str, baz => 2, qux => 3,
);
