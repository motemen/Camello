use strict;
use warnings;
use Moose;
use Smart::Args;

# Every shape the design document writes down, parsing.

has name     => (is => 'ro', isa => 'Str', required => 1);
has items    => (is => 'rw', isa => ArrayRef[Str], default => sub { [] });
has [qw(a b)] => (is => 'ro', isa => 'Int');
has '+name'  => (default => 'x');
has role     => (is => 'ro', does => 'Loggable');
has coerced  => (is => 'ro', isa => 'Int', coerce => 1);
has dict     => (is => 'ro', isa => Dict[name => Str, age => Optional[Int]]);
has slurpy   => (is => 'ro', isa => Dict[name => Str, slurpy HashRef[Str]]);
has union    => (is => 'ro', isa => 'Str|Undef');
# A class nothing in the run declares is `Unknown` and silent — but saying so
# once, at `info`, is what catches a typo in a type name.
has instance => (is => 'ro', isa => InstanceOf['IO::Handle']);
#~ info unknown-type: `IO::Handle` is not known
has typo     => (is => 'ro', isa => 'Srt');
#~ info unknown-type: `Srt` is not known
has refined  => (is => 'ro', isa => 'PositiveInt');

sub greet {
    args my $self,
         my $who   => 'Str',
         my $times => { isa => 'Int', default => 1 },
         my $loud  => { isa => Bool, optional => 1 };
    return "$who $times $loud";
}

sub at {
    args_pos my $self, my $index => 'Int';
    return $index;
}

# Returns: ArrayRef[Str]
sub listed { return [] }

# Returns: Maybe[InstanceOf['IO::Handle']]
#~ info unknown-type: `IO::Handle` is not known
sub handle { return undef }

# Returns: (Str, Int)
# Returns: Str
sub both { return ('a', 1) }

# Returns: (Str ...)
sub rows { return () }

# Returns: ()
sub nothing { return }
