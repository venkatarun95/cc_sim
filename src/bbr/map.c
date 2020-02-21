// Map for sequence numbers to sk-buffs.
// Currently implemented as a double-ended queue.
// Note: Only sequence numbers upto 2^64 - 2 are supported.

#include "map.h"

typedef struct map map;
typedef struct node node;
typedef struct sk_buff sk_buff;

sk_buff temp_sk;

// We insert at the back of the queue, and start searching for deletion from the front, like a sliding window.
// This is optimal in terms of traversal when packets arrive in-order.

// Initialize map. We create a sentinel node for the start.
void init(map* m){
    node* sentinel_entry = malloc(sizeof(node));
    sentinel_entry -> seqnum = ~0U;
    sentinel_entry -> next = sentinel_entry;
    sentinel_entry -> prev = sentinel_entry;

    m -> start = sentinel_entry;
    m -> size = 0;
}

// Create an entry for a sequence number at the end of the queue, returning the corresponding newly-made sk_buff pointer.
sk_buff* insert(map* m, u64 seqnum){
    node* entry = malloc(sizeof(node));
    
    // Previous node at the back.
    node* back = m -> start -> prev;

    // Adjust outgoing to new node.
    entry -> next = m -> start;
    entry -> prev = back;

    // Adjust incoming to new node.
    back -> next = entry;
    m -> start -> prev = entry;

    // Fill in data.    
    entry -> seqnum = seqnum;

    m -> size += 1;
    return &(entry -> skb);
}

// Delete the entry for a sequence number. As mentioned above, we start searching from the start of the queue.
void delete(map* m, u64 seqnum){

    node* curr = m -> start -> next;

    while(curr != m -> start){
        if(curr -> seqnum == seqnum) break;
        curr = curr -> next;
    }

    if(curr == m -> start) return;

    // Adjust pointers.
    node* prev = curr -> prev;
    node* next = curr -> next;

    prev -> next = next;
    next -> prev = prev;

    // Adjust size.
    m -> size -= 1;

    // Free memory.
    free(curr);
}

// Returns the corresponding node pointer for a sequence number.
node* search(map* m, u64 seqnum){
    node* curr = m -> start;
    while(curr != NULL){
        if(curr -> seqnum == seqnum) return curr;
        curr = curr -> next;
    }

    return curr;
}

// Returns the corresponding sk_buff pointer for a sequence number.
sk_buff* seqnum_to_skb(map* m, u64 seqnum){
    node* curr = search(m, seqnum);
    if(curr != NULL) return &(curr -> skb);

    return NULL;
}


// Print a representation of the map.
void print_map(map* m){
    node* curr = m -> start -> next;
    printf("In map:\n");
    while(curr != m -> start){
        printf("seqnum = %lld\n", curr -> seqnum);
        curr = curr -> next;
    }
}