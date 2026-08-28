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

# Two `Returns:` lines, one per context, in either order.
# Returns: (Str, Int)
# Returns: Maybe[Str]
sub pair { return ('a', 1) }

# The form the parenthesised one replaced, whose message shows the new one.
# Returns: Maybe[Str] | list: (Str, Int)
#~ info bad-annotation: a list shape is written
sub old_form { return ('a', 1) }

# Two answers to one question is a question nobody answered.
# Returns: Str
# Returns: Int
#~ info bad-annotation: names a scalar type twice
sub twice { return 'a' }

# Returns: ()
sub notify { return }

# Returns: ArrayRef[Str
#~ info bad-annotation: does not parse
sub broken_returns { return [] }

# Returns: list: Str
#~ info bad-annotation: a list shape is written
sub broken_list { return 'a' }

# `...` repeats one type, so there is one to repeat.
# Returns: (Str, Int ...)
#~ info bad-annotation: `...` repeats one type
sub repeated_pair { return ('a', 1) }

# A comment that is not an annotation is a comment.
# Returns nothing in particular.
sub plain { return 1 }

# The annotation wins, and the inferred shape is checked against it.
# Returns: Int
sub counted { return 'not a number'; }
#~ error return-mismatch: declared `Returns: Int`

# Returns: ()
sub silent_sub { return 1; }
#~ warning return-mismatch: declared `Returns: ()`

# A `return` inside an anonymous sub is that sub's, and nothing annotates an
# anonymous sub — so the `Returns:` above the sub it is written in has nothing
# to say about it. Left standing it reported the callback against `wrapping`.
# Returns: Int
sub wrapping {
    my $callback = sub { return 'a string' };
    $callback->();
    return 1;
}
