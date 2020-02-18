struct BBR {
	struct tcp_sock sk;
    unsigned char padding[64];
	struct rate_sample rs;
	struct map seqnum_map;
};

typedef struct BBR BBR;

void tcp_rate_skb_sent(BBR* bbr, u64 now, u64 seqnum);
void tcp_rate_gen(BBR* bbr, u64 newly_delivered, u64 in_flight, u64 rtt, u64 num_lost);
void set_state(BBR* bbr, u8 new_state);
void cwnd_event(BBR* bbr, enum tcp_ca_event ev);
void bbr_print_wrapper(BBR* bbr);
BBR* create_bbr();
void on_ack(BBR* bbr, u64 now, u64 seqnum, u64 rtt, u64 num_lost);
void on_send(BBR* bbr, u64 now, u64 seqnum);
void on_timeout(BBR* bbr);
u64 get_cwnd(BBR* bbr);
u64 get_intersend_time(BBR* bbr);
