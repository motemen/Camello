# Quotes in a type position are a *value*, not a class (`docs/types.md`,
# TYPE-3a). `Returns: 'draft' | 'live'` is the two strings a sub hands back —
# an `Enum` written the way the strings themselves are written — and reading
# it the other way made every one of them an `InstanceOf` of a class nothing
# declares, with an `unknown-type` each and a `return-mismatch` on the string
# that satisfied it.
use strict;
use warnings;

package Post;

# Returns: 'draft' | 'live'
sub status { return 'draft' }

# Any scalar meets an `Enum`, because which value it holds is a question about
# values and this checker follows shapes (TYPE-5e). A reference does not.
# Returns: 'draft' | 'live'
sub broken { return [] }
#~ warning return-mismatch: (`ArrayRef[Unknown]`) returned from a sub declared `Returns: Enum[draft, live]`

# A bareword still names a type, which is what leaves the quoted form free to
# name a value: this one is an object.
# Returns: Post
sub self_ { my $class = shift; return bless {}, $class }

# And the quotes Type::Tiny puts around a class name are read as the name they
# are, in every constructor that takes one.
# Returns: InstanceOf['Post']
sub other { return Post->self_ }
