use strict;
use warnings;

# The checker is silent when it does not know. A program with no annotations
# and no recognisable constructors gets no type diagnostics at all, and that is
# correct behaviour rather than a gap (`docs/typecheck.md`, non-goals).

package Legacy;

sub new {
    my ($class, %args) = @_;
    return bless { %args }, $class;
}

sub run {
    my $self = shift;
    my $result = $self->{handler}->($self->{input});
    return $result->{value};
}

sub describe {
    my ($self) = @_;
    return join ', ', map { "$_=$self->{$_}" } sort keys %$self;
}

package main;

my $legacy = Legacy->new(input => 'x', handler => sub { { value => 1 } });
print $legacy->run;
print $legacy->describe;
print $legacy->whatever_it_likes;
