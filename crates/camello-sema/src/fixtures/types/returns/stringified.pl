use strict;
use warnings;

# An in-place string operator leaves a string behind (`docs/types.md`,
# INFER-3d). `my $v = something_opaque(); $v =~ s/^ *//; $v` is how a corpus
# writes a sub that shapes a string, and while the operators said nothing the
# whole sub was `Unknown`.

package Slot;
use Moose;
has n => (is => 'ro', isa => 'Int');

package Shaped;

sub opaque { }

# `Carmel::App::quote`'s shape: an opaque value, chomped, substituted, handed
# back.
sub quoted {
    my $val = opaque();
    chomp $val;
    $val =~ s/^ *//;
    return $val;
}

sub substituted {
    my $v = opaque();
    $v =~ s/a/b/;
    return $v
}

sub transliterated {
    my $v = opaque();
    $v =~ tr/a/b/;
    return $v
}

sub concatenated {
    my $v = opaque();
    $v .= 'x';
    return $v
}

# A substitution only assigns where it *fires*, so a known `undef` survives
# one: `s/abc/def/` over an undef finds nothing to replace and leaves it
# undef. The reading fills in what nothing was known about; it does not
# overrule what the program said.

# Returns: Maybe[Str]
sub perhaps { return }

sub still_maybe {
    my $v = perhaps();
    $v =~ s/a/b/;
    return $v
}

# `.=` has no such caveat: the concatenation is written back either way, so
# an undef comes out of it defined.
sub concatenated_maybe {
    my $v = perhaps();
    $v .= 'x';
    return $v
}

# ----- and what says nothing, which stays `Unknown` -----

# `/r` hands back the modified copy and leaves the variable alone — the whole
# point of the flag.
sub copied {
    my $v = opaque();
    print $v =~ s/a/b/r;
    return $v;
}

# A match only reads its target. An object with an overloaded `""` is matched
# against all the time and is still an object afterwards.
sub matched {
    my $v = opaque();
    $v =~ /a/;
    return $v
}

# `chomp` is intent and not evidence: a reference survives one unchanged,
# because there is no trailing separator on it to take off.
sub chomped {
    my $v = opaque();
    chomp $v;
    return $v
}

# Not a plain variable, so not a binding this records — the rule assignment
# already holds to.
sub through_a_key {
    my $h = opaque();
    $h->{k} =~ s/a/b/;
    return $h
}

package main;

print Slot->new(n => Shaped::quoted())->n;            #~ warning type-mismatch: `Str`
print Slot->new(n => Shaped::substituted())->n;       #~ warning type-mismatch: `Str`
print Slot->new(n => Shaped::transliterated())->n;    #~ warning type-mismatch: `Str`
print Slot->new(n => Shaped::concatenated())->n;      #~ warning type-mismatch: `Str`

print Slot->new(n => Shaped::still_maybe())->n;           #~ warning type-mismatch: `Str|Undef`
print Slot->new(n => Shaped::concatenated_maybe())->n;    #~ warning type-mismatch: `Str`

print Slot->new(n => Shaped::copied())->n;
print Slot->new(n => Shaped::matched())->n;
print Slot->new(n => Shaped::chomped())->n;
print Slot->new(n => Shaped::through_a_key())->n;
