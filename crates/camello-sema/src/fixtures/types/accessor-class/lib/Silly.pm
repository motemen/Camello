# The shapes are `Class-Accessor-0.51/t/accessors.t`, `t/bestpractice.t` and
# `t/antlers.t`.
use strict;
use warnings;

package Silly;
use base 'Class::Accessor';
__PACKAGE__->mk_accessors(qw( foo bar yar ));
__PACKAGE__->mk_ro_accessors(qw(static unchanged));
__PACKAGE__->mk_wo_accessors(qw(sekret double_sekret));

package Best;
use base 'Class::Accessor::Fast';
# From here on an accessor is `get_x` / `set_x`, so the plain name is not one.
__PACKAGE__->follow_best_practice;
__PACKAGE__->mk_accessors(qw( foo ));
__PACKAGE__->mk_ro_accessors(qw(roro));
__PACKAGE__->mk_wo_accessors(qw(wowo));

package Antlers;
# `use Class::Accessor 'antlers'` is the one spelling of it that exports `has`,
# so this reads as the Moose-shaped declaration it is.
use Class::Accessor 'antlers';
has rwrw => (is => 'rw', isa => 'Int');
has roro => (is => 'ro', isa => 'Str');

package main;

my $test = Silly->new({ static => 'variable' });
$test->foo(42);
print $test->foo, $test->bar, $test->yar, $test->static, $test->unchanged;
$test->double_sekret(1001001);
print $test->nonesuch;          #~ warning unknown-method: `nonesuch`

my $best = Best->new({ foo => 'bar' });
print $best->get_foo, $best->get_roro;
$best->set_foo('x');
$best->set_wowo('y');

my $antlers = Antlers->new(rwrw => 1, roro => 'x');
print $antlers->rwrw, $antlers->roro;
# `has` carries types, so the constructor checks them — which is the whole
# difference between this spelling and the untyped `mk_accessors` above.
Antlers->new(rwrw => 'nope');   #~ error type-mismatch: `Int`
Antlers->new(typo => 1);        #~ error unknown-key: `typo`

1;
