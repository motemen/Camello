# The pairs written under a bareword call are the list's, not the call's
# arguments (`paren_less_call_along_a_list.pl`), so they sit at the level of the
# list around them. Where that list is the argument list of another such call it
# hangs under its own first argument, and the level is that column — read as
# column zero the lines went out to the margin instead, taking a comment between
# them along.
sub args;

sub f;

args my $foo => f [],
     my $bar => "";

sub wrapper {
    args my $foo => f [],
         # and further out still, once the statement itself is indented
         my $bar => "";
}
