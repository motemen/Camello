# A call whose argument is another call, with a list among its arguments. The
# elements of the inner `[ ... ]` come out no deeper than the bracket that
# opens them, and the closers `] ) )` collapse onto one line, so nothing in the
# output shows which bracket closes what.
#
# The re-indent of the outer arguments from the opening parenthesis's column to
# one level is a separate, intended change, and comes along with this input.
use Test2::V0;

sub t {
    my ($obj, $x, $y) = @_;
    my $res = $obj->foo(bar("x",
        alpha => 'a',
        bravo => [
            charlie => $x,
            delta   => $y,
        ]
    ));
    ok $res;
}
