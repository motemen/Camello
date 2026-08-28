# A value held for its destructor (`docs/types.md`, DIAG-12d). Never reading it
# is the point, and what says so is the *class*: a `DESTROY` anywhere in the
# linearisation means the end of the value's life is what it was bound for.
use strict;
use warnings;

package Lock;
sub new { my ($class) = @_; return bless {}, $class }
sub DESTROY { 1 }

package Nested;
our @ISA = ('Lock');
sub new { my ($class) = @_; return bless {}, $class }

package Plain;
sub new { my ($class) = @_; return bless {}, $class }

package main;

sub make_lock { return Lock->new }

my $held = Lock->new;
my $inherited = Nested->new;
my $from_a_sub = make_lock();

# No destructor, so never reading it is the mistake the diagnostic is about.
my $plain = Plain->new;         #~ info unused-variable: `$plain`
my $number = 42;                #~ info unused-variable: `$number`
