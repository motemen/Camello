use strict;
use warnings;

# `use constant` declares subs. What they give back is an expression this pass
# does not evaluate, so the type is `Unknown` — but the *name* is there, and a
# class whose constants were invisible answered `unknown-method` to every one.
package Shape;

use constant { SQUARE => 'square', ROUND => 'round' };
use constant SIDES    => 4;
use constant CORNERS  => qw(nw ne se sw);

sub name {
    my $class = shift;
    return $class->SQUARE;
}

package main;

my @every = (Shape->SQUARE, Shape->ROUND, Shape->SIDES, Shape->CORNERS);
my $qualified = Shape::ROUND();
my $missing = Shape->TRIANGLE;   #~ warning unknown-method: declares no method `TRIANGLE`
print "@every $qualified $missing";
