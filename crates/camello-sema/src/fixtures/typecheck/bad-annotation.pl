use strict;
use warnings;
use Moose;

# An annotation that parses says what it says.
has name  => (is => 'ro', isa => 'Str');
has items => (is => 'ro', isa => ArrayRef[Str]);
has both  => (is => 'ro', isa => 'Str|Undef');
has slot  => (is => 'ro', isa => Dict[name => Str, age => Optional[Int]]);

# An annotation that is silently ignored is worse than none.
has broken => (is => 'ro', isa => 'ArrayRef[Str');
#~ info bad-annotation: is not a type

# Code that computes a constraint is not an annotation read wrongly. The
# checker cannot read it, and says nothing about it.
my $computed = 'Str';
has late => (is => 'ro', isa => $computed);

# Returns: ArrayRef[Str]
sub items { return [] }

# Returns: Maybe[Str] | list: (Str, Int)
sub pair { return ('a', 1) }

# Returns: ()
sub notify { return }

# Returns: ArrayRef[Str
#~ info bad-annotation: does not parse
sub broken_returns { return [] }

# Returns: list: Str
#~ info bad-annotation: a `list:` shape is written
sub broken_list { return 'a' }

# A comment that is not an annotation is a comment.
# Returns nothing in particular.
sub plain { return 1 }
