# try as a function (e.g., from Try::Tiny)
try {
    do_something();
};

# try function with catch
try {
    risky();
} catch {
    handle_error($_);
};

# try function in assignment
my $result = try {
    compute();
};

# try function with multiple statements
try {
    step1();
    step2();
};
