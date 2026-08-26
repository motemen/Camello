# try used as a regular function call
try();

my $value = try();

# try as bareword function call without parentheses
try 1, 2, 3;

# try with block-like argument but treated as regular function when not followed by '{'
my $result = try $code_ref, %args;
