use strict;
use warnings;
use Dog;

my $dog = Dog->new(name => 'Rex');
#   ^ hover $dog : InstanceOf['Dog']
print $dog->speak;
#           ^ hover Dog::speak($self : Any) -> Str
#           ^ definition Dog.pm:7:5
#          ^ complete-own speak, legs, sound, rename, new, name
print $dog->name;
#           ^ definition Animal.pm:11:5

print $dog->sound;
#           ^ hover Dog::sound($self : Any) -> Str (inferred)
print $dog->rename('Rex')->speak;
#           ^ hover Dog::rename($self : Any, $name : Any) -> InstanceOf['Dog'] (inferred)
