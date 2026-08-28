# What a parameter list and an `@ISA` are allowed to look like. Both shapes
# here are ones a corpus writes constantly and the checker used to misread:
# `IO::Uncompress::Base::HeaderError` and `CPAN::FTP` are where they came from.
use strict;
use warnings;

package Debug;
sub debug { 1 }
sub shout { 2 }

# `@Qualified::ISA` names its own package rather than leaning on `our`, which
# is how a file written before `our` existed says it — and `qw(...)` is how
# most of a corpus spells the list.
package Qualified;
@Qualified::ISA = qw(Debug);
# A class that declares nothing of its own is one nothing can be said to be
# missing from, so each of these has a sub.
sub own { 1 }

# An `@ISA` may be written for a package the statement is not in.
package Setter;
@Elsewhere::ISA = ('Debug');

package Elsewhere;
sub own { 1 }

package Words;
our @ISA = qw(Debug);
sub own { 1 }

package Unreadable;
our $base = 'Debug';
our @ISA  = ($base);
sub own { 1 }

package Args;
# The invocant, taken and thrown away. It is still a parameter.
sub discards { my (undef, $name) = @_; return $name }
# A discarded slot in the middle counts too.
sub middle   { my ($self, undef, $third) = @_; return $third }
# `$_[0]` inside a string reads `@_` as surely as the bare form does, so this
# sub takes an argument its names never mentioned.
sub message  { my ($self) = shift; return "got: $_[0]" }
# Nothing else reads `@_` here, so this really does take one.
sub only     { my ($self) = shift; return 1 }

package main;

Qualified->debug;
Words->debug;
Elsewhere->debug;
Qualified->missing;             #~ warning unknown-method: `Qualified` declares no method `missing`
Words->missing;                 #~ warning unknown-method: `Words` declares no method `missing`

# An `@ISA` this pass cannot read makes the class one that might have any
# method, and nothing is reported against it.
Unreadable->whatever;

Args->discards('x');
Args->discards('x', 'y');       #~ warning arity: takes at most 2 arguments including its invocant; 3 passed
Args->middle(1, 2);
Args->middle(1, 2, 3);          #~ warning arity: takes at most 3 arguments including its invocant; 4 passed
Args->message('x');
Args->only('x');                #~ warning arity: takes at most 1 argument including its invocant; 2 passed
