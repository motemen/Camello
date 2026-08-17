warn 1
if $err;

my $x = 1
+ 2;

my $result = func($a,
$b);

sub foo {
    my $result = func($a,
$b);
}

my %hash = (key1 => 'value1',
key2 => 'value2');

warn 1
# explanation
if $err;

warn 1

if $err;

my $x = 1
# adjust
+ 2;

func(my $a,);

for my$var(@list){my$x=1;print$x;}

print $_ for@values;

print $i while $i<10;
say $x while$x>0;
print while($condition);

print $i until $i==10;
say $x until$x<=0;
print until($condition);

LOOP: while($i<10){next LOOP if $i==5;last if $i==8;redo LOOP if $flag;$i++;}

LOOP : while($i<2){}

# A label is a name, so a keyword may spell one
CHECK: {
    if(ref $data){last CHECK;}
    $data=undef;
}
sub _get_behavior {
    exists $b{$name} and return $b{$name};
    return:
}

until($i>10){$i--;}

for(;;){}
for(my $i=0;$i<10;$i++){}
for(;$i<10;){}
for($i=0;;$i++){}

...;

sub bar {
    if ($x) { ; } else { ; }
}

try {
    risky();
} catch ($err) {
    warn $err;
} finally {
    tidy_up();
}
