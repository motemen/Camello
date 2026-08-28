# What the lattice will and will not rule out between two structured types
# (`docs/types.md`, TYPE-4 and TYPE-5). Every silence here was once a
# diagnostic: the arms these exercise were missing, so two shapes that agree
# fell through to "not the same type" and became a `type-mismatch`.
use strict;
use warnings;

package Shapes;
use Smart::Args qw(args);

sub dict     { args my $class, my $d  => Dict[name => Str, age => Optional[Int]]; return $d }
sub closed   { args my $class, my $d  => Dict[name => Str, age => Int]; return $d }
sub any_hash { args my $class, my $d  => 'Dict'; return $d }
sub mapping  { args my $class, my $m  => Map[Str, Str]; return $m }
sub hashing  { args my $class, my $h  => HashRef[Str]; return $h }
sub colour   { args my $class, my $c  => Enum['red', 'green', 'blue']; return $c }
sub warm     { args my $class, my $c  => Enum['red']; return $c }
sub text     { args my $class, my $s  => 'Str'; return $s }
sub pattern  { args my $class, my $re => 'RegexpRef'; return $re }
sub matcher  { args my $class, my $re => InstanceOf['Regexp']; return $re }
#~ info unknown-type: `Regexp` is not known

package main;
use Smart::Args qw(args);

# A hash written out is a `Dict` with a slurpy (TYPE-4): nothing says the
# program will not put another key in it. Against a declared `Dict` that is
# still a fit as long as every slot the declaration names is there and agrees.
Shapes->dict(d => { name => 'x', age => 1 });
Shapes->dict(d => { name => 'x' });
Shapes->dict(d => { name => 'x', extra => 1 });
Shapes->dict(d => { name => [1] });
#~ error type-mismatch: declared `Dict[name => Str, age => Optional[Int]]`

# `age` is not optional here, but an open hash may hold it after all, so a
# written-out hash that lacks it stays silent — while a `Dict` that was
# *declared* without it is a shape that says `age` will not be there.
Shapes->closed(d => { name => 'x' });
Shapes->closed(d => only_name());
#~ warning type-mismatch: declared `Dict[name => Str, age => Int]`

# Returns: Dict[name => Str]
sub only_name { return { name => 'x' } }

# Bare `Dict` is any hash, not the empty one — a key read off it is a key, not
# an `unknown-key`.
sub keyed {
    args my $matched => 'Dict';
    return $matched->{controller};
}

# `Map[K, V]` and `HashRef[V]` are the same reference with the key side said
# or unsaid.
my %pairs = (a => 'b');
Shapes->mapping(m => \%pairs);
Shapes->hashing(h => \%pairs);
Shapes->mapping(m => { a => 'b' });

# An enum's values are strings, so it goes where a string goes; and one enum
# fits another whose values include all of its own.
sub pick {
    args my $colour => Enum['red', 'green', 'blue'],
         my $only   => Enum['red'];
    Shapes->text(s => $colour);
    Shapes->colour(c => $only);
    Shapes->warm(c => $colour);
    #~ warning type-mismatch: declared `Enum[red]`
    return;
}

# Type::Tiny has one regexp type under two names.
sub matching {
    args my $re => 'RegexpRef';
    Shapes->pattern(re => $re);
    Shapes->matcher(re => $re);
    return;
}

# Arithmetic on two integers is an integer (INFER-1a). `$limit + 1` reading as
# `Num` was a `type-mismatch` against every `Int` slot it was written for.
package Counts;
use Smart::Args qw(args);
sub want_int { args my $class, my $n => 'Int'; return $n }
sub want_num { args my $class, my $n => 'Num'; return $n }

package main;

sub counted {
    args my $limit => 'Int',
         my $ratio => 'Num';
    Counts->want_int(n => $limit + 1);
    Counts->want_int(n => $limit * 2 - 1);
    Counts->want_int(n => $ratio % 10);
    Counts->want_num(n => $limit / 2);
    # `/` and `**` are not closed over the integers, so neither is `Int`.
    Counts->want_int(n => $limit / 2);
    #~ warning type-mismatch: declared `Int`
    Counts->want_int(n => $limit ** 2);
    #~ warning type-mismatch: declared `Int`
    Counts->want_int(n => $ratio + 1);
    #~ warning type-mismatch: declared `Int`
    # An operand nobody typed leaves the answer untyped rather than `Num`.
    Counts->want_int(n => whatever() + 1);
    return;
}

sub whatever { return }
