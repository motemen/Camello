# `use Moose` imports `has` into the package it is written in, so what a file
# can be expected to mean is a question about a *package* and not about the
# file (`docs/types.md`, ANNOT-1a). Read as a file's, a second package here
# was handed Moose's `has`, Moose's attributes and Moose's `new` — and
# `Plain->new(...)` became an `unknown-key` error against a constructor that
# does not exist.
use strict;
use warnings;

package Attributed;
use Moose;
has thing => (is => 'ro', isa => 'Int');

package Plain;
# No framework here. `has` is this package's own sub, and what it does with
# its argument is its own business.
sub has     { return 1 }
sub new     { my ($class) = @_; return bless {}, $class }
has 'not_an_attribute';

package main;

Attributed->new(thing => 1);
Attributed->new(nope => 1);     #~ error unknown-key: declares no attribute `nope`

my $plain = Plain->new;
$plain->missing;                #~ warning unknown-method: declares no method `missing`
