# The space just inside a bracket depends on how many elements it holds: one
# element and it is removed, two and it stays. The last line shows both at
# once — the outer `[ 200, ... ]` keeps its spaces and the `[ foo($body) ]`
# inside it loses them.
#
# None of these lines should change.
use Test2::V0;

my ($list, $ref, $obj, $body);
my $x = [ map { $_->foo } @$list ];
my $y = @{ $ref->{bar} };
ok dies { baz() };
my $z = { $obj->qux };
my $w = [ 200, [ 'a' => 'b' ], [ foo($body) ] ];
