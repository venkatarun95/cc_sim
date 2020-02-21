struct sk_buff;

struct node {
    struct node* next;
    struct node* prev;
    struct sk_buff skb;
    u64 seqnum;
};

struct map {
    int size;
    struct node* start;
};
