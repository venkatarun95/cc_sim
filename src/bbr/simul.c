#include "linux_bbr/src/tcp_bbr.c"
#include "map.c"
#include "simul.h"
#include <stdio.h>
#include <stdlib.h>

// Constants.
const u64 PACKET_SIZE = 1500;

/* Functions modified from the Linux kernel. */
void tcp_rate_skb_sent(BBR* bbr, u64 now_ns, u64 seqnum)
{
	struct sk_buff *skb = insert(&(bbr -> seqnum_map), seqnum);
	skb -> skb_mstamp_ns = now_ns;

	 /* In general we need to start delivery rate samples from the
	  * time we received the most recent ACK, to ensure we include
	  * the full time the network needs to deliver all in-flight
	  * packets. If there are no packets in flight yet, then we
	  * know that any ACKs after now indicate that the network was
	  * able to deliver those packets completely in the sampling
	  * interval between now and the next ACK.
	  *
	  * Note that we use packets_out instead of tcp_packets_in_flight(tp)
	  * because the latter is a guess based on RTO and loss-marking
	  * heuristics. We don't want spurious RTOs or loss markings to cause
	  * a spuriously small time interval, causing a spuriously high
	  * bandwidth estimate.
	  */
	if (!bbr -> sk.packets_out) {
		u64 tstamp_us = tcp_skb_timestamp_us(skb);
		bbr -> sk.first_tx_mstamp  = tstamp_us;
		bbr -> sk.delivered_mstamp = tstamp_us;
	}

	TCP_SKB_CB(skb)->tx.first_tx_mstamp	 = bbr -> sk.first_tx_mstamp;
	TCP_SKB_CB(skb)->tx.delivered_mstamp = bbr -> sk.delivered_mstamp;
	TCP_SKB_CB(skb)->tx.delivered		 = bbr -> sk.delivered;
	TCP_SKB_CB(skb)->tx.is_app_limited	 = bbr -> sk.app_limited ? 1 : 0;
}

/* When an skb is sacked or acked, we fill in the rate sample with the (prior)
 * delivery information when the skb was last transmitted.
 *
 * If an ACK (s)acks multiple skbs (e.g., stretched-acks), this function is
 * called multiple times. We favor the information from the most recently
 * sent skb, i.e., the skb with the highest prior_delivered count.
 */
void tcp_rate_skb_delivered(BBR* bbr, u64 now, u64 seqnum)
{
    struct sk_buff *skb = seqnum_to_skb(&(bbr -> seqnum_map), seqnum); 
	struct tcp_skb_cb *scb = TCP_SKB_CB(skb);

	if (!scb->tx.delivered_mstamp){
		// printf("BRO! NOT FILLED!");
        return;
    }

	if (!bbr -> rs.prior_delivered || after(scb->tx.delivered, bbr -> rs.prior_delivered)) {
		bbr -> rs.prior_delivered  = scb->tx.delivered;
		bbr -> rs.prior_mstamp     = scb->tx.delivered_mstamp;
		bbr -> rs.is_app_limited   = scb->tx.is_app_limited;
		bbr -> rs.is_retrans	   = 0;
		// bbr -> rs.is_retrans	   = scb->sacked & TCPCB_RETRANS;

		/* Record send time of most recently ACKed packet: */
		bbr -> sk.first_tx_mstamp  = tcp_skb_timestamp_us(skb);
		/* Find the duration of the "send phase" of this window: */
		bbr -> rs.interval_us = tcp_stamp_us_delta(bbr -> sk.first_tx_mstamp, scb->tx.first_tx_mstamp);
	}
	/* Mark off the skb delivered once it's sacked to avoid being
	 * used again when it's cumulatively acked. For acked packets
	 * we don't need to reset since it'll be freed soon.
	 */
	// if (scb->sacked & TCPCB_SACKED_ACKED)
	if (scb->sacked){
		scb->tx.delivered_mstamp = 0;
	}
}

/* Update the connection delivery information and generate a rate sample. */
void tcp_rate_gen(BBR* bbr, u64 newly_delivered, u64 rtt, u64 num_lost){
    
    /* Clear app limited if bubble is acked and gone. */
	if (bbr -> sk.app_limited && after(bbr -> sk.delivered, bbr -> sk.app_limited)){
		bbr -> sk.app_limited = 0;
    }

	/* TODO: there are multiple places throughout tcp_ack() to get
	 * current time. Refactor the code using a new "tcp_acktag_state"
	 * to carry current time, flags, stats like "tcp_sacktag_state".
	 */
	if (newly_delivered){
		bbr -> sk.delivered_mstamp = bbr -> sk.tcp_mstamp;
    }

	bbr -> rs.acked_sacked = newly_delivered;	/* freshly ACKed or SACKed */
	bbr -> rs.losses = num_lost;				/* freshly marked lost */
	/* Return an invalid sample if no timing information is available or
	 * in recovery from loss with SACK reneging. Rate samples taken during
	 * a SACK reneging event may overestimate bw by including packets that
	 * were SACKed before the reneg.
	 */
	if (!bbr -> rs.prior_mstamp) {
		bbr -> rs.delivered = -1;
		bbr -> rs.interval_us = -1;
		bbr -> rs.rtt_us = -1;
		return;
	}

	bbr -> rs.delivered = bbr -> sk.delivered - bbr -> rs.prior_delivered;

	/* Model sending data and receiving ACKs as separate pipeline phases
	 * for a window. Usually the ACK phase is longer, but with ACK
	 * compression the send phase can be longer. To be safe we use the
	 * longer phase.
	 */
	u32 snd_us = bbr -> rs.interval_us;				/* send phase */
	u32 ack_us = tcp_stamp_us_delta(bbr -> sk.tcp_mstamp, bbr -> rs.prior_mstamp); /* ack phase */
	bbr -> rs.interval_us = max(snd_us, ack_us);

	/* Record both segment send and ack receive intervals */
	bbr -> rs.snd_interval_us = snd_us;
	bbr -> rs.rcv_interval_us = ack_us;

    bbr -> rs.rtt_us = rtt;

	/* Normally we expect interval_us >= min-rtt.
	 * Note that rate may still be over-estimated when a spuriously
	 * retransmistted skb was first (s)acked because "interval_us"
	 * is under-estimated (up to an RTT). However continuously
	 * measuring the delivery rate during loss recovery is crucial
	 * for connections suffer heavy or prolonged losses.
	 */
	if (unlikely(bbr -> rs.interval_us < tcp_min_rtt(&(bbr -> sk)))) {
		// if (!bbr -> rs.is_retrans)
		// 	pr_debug("tcp rate: %ld %d %u %u %u\n",
		// 		 bbr -> rs.interval_us, bbr -> rs.delivered,
		// 		 inet_csk(sk)->icsk_ca_state,
		// 		 bbr -> sk.rx_opt.sack_ok, tcp_min_rtt(tp));
		// bbr -> rs.interval_us = -1;
		return;
	}

	/* Record the last non-app-limited or the highest app-limited bw */
	if (!bbr -> rs.is_app_limited || ((u64)bbr -> rs.delivered * bbr -> sk.rate_interval_us >= (u64)bbr -> sk.rate_delivered * bbr -> rs.interval_us)) {
		bbr -> sk.rate_delivered = bbr -> rs.delivered;
		bbr -> sk.rate_interval_us = bbr -> rs.interval_us;
		bbr -> sk.rate_app_limited = bbr -> rs.is_app_limited;
	}
}

/* Functions I've written from scratch. */
void set_state(BBR* bbr, u8 new_state){
    bbr_set_state(&(bbr -> sk), new_state);
}

void cwnd_event(BBR* bbr, enum tcp_ca_event ev){
    bbr_cwnd_event(&(bbr -> sk), ev);
}

void bbr_print_wrapper(BBR* bbr){
	bbr_print(&(bbr -> sk));
}

// Create and initialize a new BBR instance.
BBR* create_bbr(){
	BBR* bbr = malloc(sizeof(BBR));
	memset(&(bbr -> sk), 0, sizeof(bbr -> sk));

	bbr -> sk.srtt_us = 0;
    bbr -> sk.snd_cwnd = TCP_INIT_CWND;
    bbr -> sk.mss_cache = TCP_BASE_MSS;
    bbr -> sk.snd_cwnd_clamp = ~0U;
    bbr -> sk.sk_pacing_rate = 0U;
    bbr -> sk.sk_max_pacing_rate = ~0U;
	bbr -> sk.delivered = 0;
	bbr -> sk.delivered_mstamp = 0;
	
	minmax_reset(&bbr -> sk.rtt_min, tcp_jiffies32(&bbr -> sk), ~0U);
    
    bbr_init(&(bbr -> sk));
	
    set_state(bbr, TCP_CA_Open);
    cwnd_event(bbr, CA_EVENT_TX_START);

	return bbr;
}

// Initialization for rate_sample.
struct rate_sample create_empty_rate_sample(){
	struct rate_sample rs = {0};
	rs.delivered = 0;
	rs.prior_delivered = 0;
	rs.rtt_us = -1;
	return rs;
}

/* Functions we have to implement for the simulator. */
// / Called each time an ack arrives. `now` and `ack_seq` are enough to compute `rtt` and
// / `num_lost`. They are provided separately for convenience. `loss` denotes the number of
// / packets that were lost.
void on_ack(BBR* bbr, u64 now, u64 seqnum, u64 rtt, u64 num_lost){
    cwnd_event(bbr, CA_EVENT_FAST_ACK);

	u64 newly_delivered = seqnum / PACKET_SIZE - bbr -> sk.delivered;

	u64 now_ns = now * NSEC_PER_USEC;
    bbr -> sk.tcp_mstamp = now_ns;
    bbr -> sk.delivered_mstamp = now_ns;
    bbr -> sk.delivered = seqnum / PACKET_SIZE;
	
	tcp_rate_skb_delivered(bbr, now_ns, seqnum);
	tcp_rate_gen(bbr, newly_delivered, rtt, num_lost);
    
	bbr_main(&(bbr -> sk), &(bbr -> rs));
}


/// Called each time a packet is sent
void on_send(BBR* bbr, u64 now, u64 seqnum){

    bbr -> sk.intersend_time = min(bbr -> sk.intersend_time, now - bbr -> sk.send_timestamp_us);
    bbr -> sk.send_timestamp_us = now;

	bbr -> rs = create_empty_rate_sample();
	tcp_rate_skb_sent(bbr, now, seqnum);
    
    bbr -> sk.tcp_mstamp = now;
	bbr -> sk.packets_out = seqnum / PACKET_SIZE;
}

/// Called if the sender timed out
void on_timeout(BBR* bbr){
    cwnd_event(bbr, CA_EVENT_LOSS);
}

/// The congestion window (in packets)
u64 get_cwnd(BBR* bbr){
    return bbr -> sk.snd_cwnd;
}

/// Returns the minimum interval between any two transmitted packets
u64 get_intersend_time(BBR* bbr){
    return bbr -> sk.intersend_time;
}


// Simple send-and-receive test loop.
void loop(BBR* bbr){
	u64 seqnum = 0;
	u64 rtt = ~0U;

    for(u64 now = 1000; now < 10000; now += 100){
		seqnum += PACKET_SIZE;
		rtt = 40 + rand() % 20;

		printf("Time = %lld ms:\n", now);
		printf("Sending segment with seqnum = %lld...\n", seqnum);
        on_send(bbr, now, seqnum);
		bbr_print_wrapper(bbr);

		printf("Time = %lld ms:\n", now + rtt);
		printf("Received ACK for segment with seqnum = %lld...\n", seqnum);
        on_ack(bbr, now + rtt, seqnum, rtt, 0);
		bbr_print_wrapper(bbr);

		printf("--------\n");
    }
}

// int main(){
// 	printf("Welcome to Ameya's hack for TCP-BBR!\n");
// 	printf("Initializing...\n\n");
	
// 	BBR* bbr = create_bbr();
// 	bbr_print_wrapper(bbr);
// 	printf("--------\n");
	
// 	loop(bbr);
// }