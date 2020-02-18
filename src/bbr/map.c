// Map for sequence numbers to sk-buffs.
// Currently implemented as a simple linked-list.
#include "map.h"

typedef struct map map;
typedef struct node node;
typedef struct sk_buff sk_buff;

sk_buff temp_sk;

// Create an entry for a sequence number, returning the corresponding newly-made sk_buff pointer.
sk_buff* insert(map* m, u64 seqnum){
    node* entry = malloc(sizeof(node));
    entry -> next = m -> start;
    entry -> seqnum = seqnum;
    m -> start = entry;
    m -> size += 1;
    return &(entry -> skb);
}

// Delete the entry for a sequence number.
void delete(map* m, u64 seqnum){
    node* curr = m -> start;
    node* prev = NULL;

    while(curr != NULL){
        if(curr -> seqnum == seqnum){
            break;
        }

        prev = curr;
        curr = curr -> next;
    }

    if(curr == NULL){
        return;
    }

    // Adjust size.
    m -> size -= 1;

    // Adjust pointers.
    if(prev != NULL){
        prev -> next = curr -> next;
    }
    
    // Free memory.
    free(curr);
}

// Returns the corresponding node pointer for a sequence number.
node* search(map* m, u64 seqnum){
    node* curr = m -> start;
    while(curr != NULL){
        if(curr -> seqnum == seqnum){
            return curr;
        }
        curr = curr -> next;
    }

    return curr;
}

// Returns the corresponding sk_buff pointer for a sequence number.
sk_buff* seqnum_to_skb(map* m, u64 seqnum){
    node* curr = search(m, seqnum);
    if(curr != NULL){
        return &(curr -> skb);
    }

    return NULL;
}


// Print a representation of the map.
void print_map(map* m){
    node* curr = m -> start;
    printf("In map:\n");
    while(curr != NULL){
        printf("seqnum = %lld\n", curr -> seqnum);
        curr = curr -> next;
    }
}