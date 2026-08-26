use strict;
use warnings;
use Moose;

has name  => (is => 'ro', isa => 'Str', required => 1);
has count => (is => 'ro', isa => 'Int');
has items => (is => 'ro', isa => ArrayRef[Str]);

package Widget;
use Moose;
has label => (is => 'ro', isa => 'Str');
sub render { return 1 }

package main;

# A constructor takes a Dict of the attributes it declares.
my $widget = Widget->new(label => 'x');
Widget->new(label => [1]);      #~ error type-mismatch: declared `Str`
Widget->new(nope => 1);         #~ error unknown-key: declares no attribute `nope`

# A method the class declares resolves; one it does not is a warning, because
# the class may be right and the program wrong about what it holds.
$widget->render;
$widget->label;
$widget->missing;               #~ warning unknown-method: declares no method `missing`

# `Unknown` propagates: an operation on something nobody typed says nothing.
my $opaque = get_it();
$opaque->anything_at_all;
sub get_it { return }
