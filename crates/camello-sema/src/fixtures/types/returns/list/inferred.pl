use strict;
use warnings;

package Row;
use Moose;
has id => (is => 'ro', isa => 'Int');

package Store;

# The list half of a site, with no annotation anywhere. Both halves go through
# the tiers together: a shape is a value in `Returns` like the scalar type is.

sub pair { return (Row->new(id => 1), 'x') }

sub rows {
    my @rows = (Row->new(id => 1));
    return @rows;
}

# Transitive, which is the point of the shape being a site: `mapped` cannot be
# read until `rows` has been.
sub mapped { return map { $_ } rows() }

sub kept { return grep { $_->id } rows() }

# `return;` is an empty list, so a sub with one of those among its list
# returns does not know how many come back — and a single target off it may be
# the one that was not there.
sub perhaps {
    my ($self, $ok) = @_;
    return unless $ok;
    return Row->new(id => 1);
}

package main;

my ($row, $name) = Store::pair();
$row->nope;                     #~ warning unknown-method: `Row` declares no method `nope`
print $name;

for my $listed (Store::rows()) {
    $listed->nope;                 #~ warning unknown-method: `Row` declares no method `nope`
}

for my $mapped (Store::mapped()) {
    $mapped->nope;                 #~ warning unknown-method: `Row` declares no method `nope`
}

for my $kept (Store::kept()) {
    $kept->nope;                 #~ warning unknown-method: `Row` declares no method `nope`
}

my ($maybe) = Store->perhaps(1);
$maybe->id;                     #~ info maybe-deref: may be undefined here

# And the scalar half of the same sub is not a `Maybe` of a list but the join
# of its scalar sites.
my $scalar = Store->perhaps(1);
$scalar->id;                    #~ info maybe-deref: may be undefined here
