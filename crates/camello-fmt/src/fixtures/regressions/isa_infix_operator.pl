# `isa` is an infix operator whose right operand is a bareword class name, not
# an expression. It binds tighter than `&&` and looser than a comparison, and it
# appears in all three of the positions below.
use v5.36;

sub foo {
    my ($x) = @_;

    return !!0 unless $x isa Bar;

    if ($x isa Bar::Baz && $x->qux) {
        return 1;
    }

    my $y = $x isa Bar ? $x
        : Bar->new('quux', {}, $x . q());

    return $y;
}

# ... and is still a name wherever the grammar wants one.
sub isa { return 0 }

my %h = (isa => 1);
my $z = Bar->isa('Baz') || $h{isa};
