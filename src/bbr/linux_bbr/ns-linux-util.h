/* 
 * TCP-Linux module for NS2 
 *
 * May 2006
 *
 * Author: Xiaoliang (David) Wei  (DavidWei@acm.org)
 *
 * NetLab, the California Institute of Technology 
 * http://netlab.caltech.edu
 *
 * Module: linux/ns-linux-util.h
 *      This is the header file for linkages between NS-2 source codes (in C++) and Linux source codes (in C)
 *
 * See a mini-tutorial about TCP-Linux at: http://netlab.caltech.edu/projects/ns2tcplinux/
 *
 */

#ifndef NS_LINUX_UTIL_H
#define NS_LINUX_UTIL_H
#include "ns-linux-param.h"
#include <stdlib.h>
#include <stdint.h>
#include <stdbool.h>

extern struct tcp_congestion_ops tcp_reno;
extern long long ktime_get_real;

#define JIFFY_RATIO 1000
#define US_RATIO 1000000
#define MS_RATIO 1000

#define tcp_jiffies32(sk) (tcp_time_stamp(sk))
#define TCP_TS_HZ	1000

#define jiffies_to_usecs(x) ((US_RATIO/JIFFY_RATIO)*(x))
#define msecs_to_jiffies(x) ((JIFFY_RATIO/MS_RATIO)*(x))

extern void tcp_cong_avoid_register(void);

#define __u64 unsigned long long
#define __u32 unsigned long
#define __u16 unsigned int
#define __u8 unsigned char

#define u64 __u64
#define u32 __u32
#define u16 __u16
#define u8 __u8

#define s32 long
#define s64 long long

#define __be16 __u16
#define __be32 __u32

#define ktime_t s64
extern ktime_t net_invalid_timestamp();
extern int ktime_equal(const ktime_t cmp1, const ktime_t cmp2);
extern s64 ktime_to_us(const ktime_t kt);
extern ktime_t net_timedelta(ktime_t t);

#define inet_csk(sk) (sk)
#define tcp_sk(sk) (sk)
#define inet_csk_ca(sk) (void*)((sk)->icsk_ca_priv)

#define TCP_INFINITE_SSTHRESH	0x7fffffff

/* SPDX-License-Identifier: GPL-2.0 WITH Linux-syscall-note */
#ifndef _UAPI_INET_DIAG_H_
#define _UAPI_INET_DIAG_H_

#define likely(x)       __builtin_expect(!!(x), 1)
#define unlikely(x)     __builtin_expect(!!(x), 0)


/* Just some random number */
#define TCPDIAG_GETSOCK 18
#define DCCPDIAG_GETSOCK 19

#define INET_DIAG_GETSOCK_MAX 24


/* Socket identity */
struct inet_diag_sockid {
	__be16	idiag_sport;
	__be16	idiag_dport;
	__be32	idiag_src[4];
	__be32	idiag_dst[4];
	__u32	idiag_if;
	__u32	idiag_cookie[2];
#define INET_DIAG_NOCOOKIE (~0U)
};


/*
 * SOCK_RAW sockets require the underlied protocol to be
 * additionally specified so we can use @pad member for
 * this, but we can't rename it because userspace programs
 * still may depend on this name. Instead lets use another
 * structure definition as an alias for struct
 * @inet_diag_req_v2.
 */
struct inet_diag_req_raw {
	__u8	sdiag_family;
	__u8	sdiag_protocol;
	__u8	idiag_ext;
	__u8	sdiag_raw_protocol;
	__u32	idiag_states;
	struct inet_diag_sockid id;
};

enum {
	INET_DIAG_REQ_NONE,
	INET_DIAG_REQ_BYTECODE,
};

#define INET_DIAG_REQ_MAX INET_DIAG_REQ_BYTECODE

/* Bytecode is sequence of 4 byte commands followed by variable arguments.
 * All the commands identified by "code" are conditional jumps forward:
 * to offset cc+"yes" or to offset cc+"no". "yes" is supposed to be
 * length of the command and its arguments.
 */
 
struct inet_diag_bc_op {
	unsigned char	code;
	unsigned char	yes;
	unsigned short	no;
};

enum {
	INET_DIAG_BC_NOP,
	INET_DIAG_BC_JMP,
	INET_DIAG_BC_S_GE,
	INET_DIAG_BC_S_LE,
	INET_DIAG_BC_D_GE,
	INET_DIAG_BC_D_LE,
	INET_DIAG_BC_AUTO,
	INET_DIAG_BC_S_COND,
	INET_DIAG_BC_D_COND,
	INET_DIAG_BC_DEV_COND,   /* u32 ifindex */
	INET_DIAG_BC_MARK_COND,
	INET_DIAG_BC_S_EQ,
	INET_DIAG_BC_D_EQ,
};

struct inet_diag_hostcond {
	__u8	family;
	__u8	prefix_len;
	int	port;
	__be32	addr[0];
};

struct inet_diag_markcond {
	__u32 mark;
	__u32 mask;
};

/* Base info structure. It contains socket identity (addrs/ports/cookie)
 * and, alas, the information shown by netstat. */
struct inet_diag_msg {
	__u8	idiag_family;
	__u8	idiag_state;
	__u8	idiag_timer;
	__u8	idiag_retrans;

	struct inet_diag_sockid id;

	__u32	idiag_expires;
	__u32	idiag_rqueue;
	__u32	idiag_wqueue;
	__u32	idiag_uid;
	__u32	idiag_inode;
};

/* Extensions */

enum {
	INET_DIAG_NONE,
	INET_DIAG_MEMINFO,
	INET_DIAG_INFO,
	INET_DIAG_VEGASINFO,
	INET_DIAG_CONG,
	INET_DIAG_TOS,
	INET_DIAG_TCLASS,
	INET_DIAG_SKMEMINFO,
	INET_DIAG_SHUTDOWN,

	/*
	 * Next extenstions cannot be requested in struct inet_diag_req_v2:
	 * its field idiag_ext has only 8 bits.
	 */

	INET_DIAG_DCTCPINFO,	/* request as INET_DIAG_VEGASINFO */
	INET_DIAG_PROTOCOL,	/* response attribute only */
	INET_DIAG_SKV6ONLY,
	INET_DIAG_LOCALS,
	INET_DIAG_PEERS,
	INET_DIAG_PAD,
	INET_DIAG_MARK,		/* only with CAP_NET_ADMIN */
	INET_DIAG_BBRINFO,	/* request as INET_DIAG_VEGASINFO */
	INET_DIAG_CLASS_ID,	/* request as INET_DIAG_TCLASS */
	INET_DIAG_MD5SIG,
	INET_DIAG_ULP_INFO,
	__INET_DIAG_MAX,
};

#define INET_DIAG_MAX (__INET_DIAG_MAX - 1)

enum {
	INET_ULP_INFO_UNSPEC,
	INET_ULP_INFO_NAME,
	INET_ULP_INFO_TLS,
	__INET_ULP_INFO_MAX,
};
#define INET_ULP_INFO_MAX (__INET_ULP_INFO_MAX - 1)

/* INET_DIAG_MEM */

struct inet_diag_meminfo {
	__u32	idiag_rmem;
	__u32	idiag_wmem;
	__u32	idiag_fmem;
	__u32	idiag_tmem;
};

/* INET_DIAG_VEGASINFO */

struct tcpvegas_info {
	__u32	tcpv_enabled;
	__u32	tcpv_rttcnt;
	__u32	tcpv_rtt;
	__u32	tcpv_minrtt;
};

/* INET_DIAG_DCTCPINFO */

struct tcp_dctcp_info {
	__u16	dctcp_enabled;
	__u16	dctcp_ce_state;
	__u32	dctcp_alpha;
	__u32	dctcp_ab_ecn;
	__u32	dctcp_ab_tot;
};

/* INET_DIAG_BBRINFO */

struct tcp_bbr_info {
	/* u64 bw: max-filtered BW (app throughput) estimate in Byte per sec: */
	__u32	bbr_bw_lo;		/* lower 32 bits of bw */
	__u32	bbr_bw_hi;		/* upper 32 bits of bw */
	__u32	bbr_min_rtt;		/* min-filtered RTT in uSec */
	__u32	bbr_pacing_gain;	/* pacing gain shifted left 8 bits */
	__u32	bbr_cwnd_gain;		/* cwnd gain shifted left 8 bits */
};

union tcp_cc_info {
	struct tcpvegas_info	vegas;
	struct tcp_dctcp_info	dctcp;
	struct tcp_bbr_info	bbr;
};
#endif /* _UAPI_INET_DIAG_H_ */

//from kernel.h
#define min_t(type,x,y) \
	(((type)(x)) < ((type)(y)) ? ((type)(x)): ((type)(y)))

#define max_t(type,x,y) \
	(((type)(x)) > ((type)(y)) ? ((type)(x)): ((type)(y)))

// from time64.h
/* Parameters used to convert the timespec values: */
#define MSEC_PER_SEC	1000L
#define USEC_PER_MSEC	1000L
#define NSEC_PER_USEC	1000L
#define NSEC_PER_MSEC	1000000L
#define USEC_PER_SEC	1000000L
#define NSEC_PER_SEC	1000000000L
#define FSEC_PER_SEC	1000000000000000LL

/* Located here for timespec[64]_valid_strict */
#define TIME64_MAX			((s64)~((u64)1 << 63))
#define KTIME_MAX			((s64)~((u64)1 << 63))
#define KTIME_SEC_MAX			(KTIME_MAX / NSEC_PER_SEC)

//#define max(x,y) ((x>y)? x:y)

/* Events passed to congestion control interface */
enum tcp_ca_event {
	CA_EVENT_TX_START,	/* first transmit when no packets in flight */
	CA_EVENT_CWND_RESTART,	/* congestion window restart */
	CA_EVENT_COMPLETE_CWR,	/* end of congestion recovery */
	CA_EVENT_FRTO,		/* fast recovery timeout */
	CA_EVENT_LOSS,		/* loss timeout */
	CA_EVENT_FAST_ACK,	/* in sequence ack */
	CA_EVENT_SLOW_ACK,	/* other ack */
};

#define sock tcp_sock
#define inet_sock tcp_sock
#define inet_connection_sock tcp_sock
struct sk_buff;
struct sock;


/* A list of parameters for different cc */
struct cc_param_list {
	const char* name;
	const char* type;
	const char* description;
	const void* ptr;
	struct cc_param_list* next;
};

struct cc_list {
	const char* proto;
	struct cc_param_list* param_head;
	struct cc_list* next;
};
extern struct cc_list* cc_list_head;

/* List Type purely for tcp_cong_list*/
struct list_head {
	struct list_head *prev;
	struct list_head *next;
	const char* file_name;
};

extern unsigned char cc_list_changed;
extern struct list_head ns_tcp_cong_list;
extern struct list_head *last_added;
#define list_for_each_entry_rcu(pos, head, member) \
	for (pos=(struct tcp_congestion_ops*)ns_tcp_cong_list.next; pos!=(struct tcp_congestion_ops*)(&ns_tcp_cong_list); pos=(struct tcp_congestion_ops*)pos->member.next) 

#define list_add_rcu(a,b) {\
	(a)->next=ns_tcp_cong_list.next;\
	(a)->prev=(&ns_tcp_cong_list);\
	ns_tcp_cong_list.next->prev=(a);\
	ns_tcp_cong_list.next=(a);\
	last_added = (a);\
	cc_list_changed = 1;\
}

#define list_del_rcu(a) {(a)->prev->next=(a)->next; (a)->next->prev=(a)->prev; cc_list_changed = 1; }
#define list_move(a,b) {list_del_rcu(a); list_add_rcu(a,b);}
#define list_entry(a,b,c) ((b*)ns_tcp_cong_list.next)
#define list_add_tail_rcu(a,b) {\
	(a)->prev=ns_tcp_cong_list.prev;\
	(a)->next=(&ns_tcp_cong_list);\
	ns_tcp_cong_list.prev->next=(a);\
	ns_tcp_cong_list.prev=(a);\
	last_added = (a);\
	cc_list_changed = 1;\
}

/*
 * Interface for adding new TCP congestion control handlers
 */
#define TCP_CA_NAME_MAX	16
#define TCP_CONG_NON_RESTRICTED 0x1
#define TCP_CONG_RTT_STAMP      0x2
struct tcp_congestion_ops {
	struct list_head	list;

	unsigned long flags;
	int non_restricted;

	/* initialize private data (optional) */
	void (*init)(struct sock *sk);
	/* cleanup private data  (optional) */
	void (*release)(struct sock *sk);

	/* return slow start threshold (required) */
	u32 (*ssthresh)(struct sock *sk);
	/* lower bound for congestion window (optional) */
	u32 (*min_cwnd)(const struct sock *sk);
	/* do new cwnd calculation (required) */
	void (*cong_avoid)(struct sock *sk, u32 ack,
			   u32 rtt, u32 in_flight, int good_ack);
	/* round trip time sample per acked packet (optional) */
	void (*rtt_sample)(struct sock *sk, u32 usrtt);
	/* call before changing ca_state (optional) */
	void (*set_state)(struct sock *sk, u8 new_state);
	/* call when cwnd event occurs (optional) */
	void (*cwnd_event)(struct sock *sk, enum tcp_ca_event ev);
	/* new value of cwnd after loss (optional) */
	u32  (*undo_cwnd)(struct sock *sk);
	/* hook for packet ack accounting (optional) */
	void (*pkts_acked)(struct sock *sk, u32 num_acked, ktime_t last);
	/* get info for inet_diag (optional) */
	void (*get_info)(struct sock *sk, u32 ext, struct sk_buff *skb);
	/* call when packets are delivered to update cwnd and pacing rate,
	 * after all the ca_state processing. (optional)
	 */
	void (*cong_control)(struct sock *sk, const struct rate_sample *rs);
	/* returns the multiplier used in tcp_sndbuf_expand (optional) */
	u32 (*sndbuf_expand)(struct sock *sk);
	/* override sysctl_tcp_min_tso_segs */
	u32 (*min_tso_segs)(struct sock *sk);

	char 		name[TCP_CA_NAME_MAX];
	struct module 	*owner;
};

struct tcp_options_received {
/*	PAWS/RTTM data	*/
//	long	ts_recent_stamp;/* Time we stored ts_recent (for aging) */
//	__u32	ts_recent;	/* Time stamp to echo next		*/
	__u32	rcv_tsval;	/* Time stamp value             	*/
	__u32	rcv_tsecr;	/* Time stamp echo reply        	*/
	__u16 	saw_tstamp : 1,	/* Saw TIMESTAMP on last packet		*/
		dump_xxx: 15;
//		tstamp_ok : 1,	/* TIMESTAMP seen on SYN packet		*/
//		dsack : 1,	/* D-SACK is scheduled			*/
//		wscale_ok : 1,	/* Wscale seen on SYN packet		*/
//		sack_ok : 4,	/* SACK seen on SYN packet		*/
//		snd_wscale : 4,	/* Window scaling received from sender	*/
//		rcv_wscale : 4;	/* Window scaling to send to receiver	*/
/*	SACKs data	*/
//	__u8	eff_sacks;	/* Size of SACK array to send with next packet */
//	__u8	num_sacks;	/* Number of SACK blocks		*/
///	__u16	user_mss;  	/* mss requested by user in ioctl */
//	__u16	mss_clamp;	/* Maximal mss, negotiated at connection setup */
};

// SPDX-License-Identifier: GPL-2.0
/**
 * lib/minmax.c: windowed min/max tracker
 *
 * Kathleen Nichols' algorithm for tracking the minimum (or maximum)
 * value of a data stream over some fixed time interval.  (E.g.,
 * the minimum RTT over the past five minutes.) It uses constant
 * space and constant time per update yet almost always delivers
 * the same minimum as an implementation that has to keep all the
 * data in the window.
 *
 * The algorithm keeps track of the best, 2nd best & 3rd best min
 * values, maintaining an invariant that the measurement time of
 * the n'th best >= n-1'th best. It also makes sure that the three
 * values are widely separated in the time window since that bounds
 * the worse case error when that data is monotonically increasing
 * over the window.
 *
 * Upon getting a new min, we can forget everything earlier because
 * it has no value - the new min is <= everything else in the window
 * by definition and it's the most recent. So we restart fresh on
 * every new min and overwrites 2nd & 3rd choices. The same property
 * holds for 2nd & 3rd best.
 */
/* SPDX-License-Identifier: GPL-2.0 */
/**
 * lib/minmax.c: windowed min/max tracker by Kathleen Nichols.
 *
 */

#ifndef MINMAX_H
#define MINMAX_H


/* A single data point for our parameterized min-max tracker */
struct minmax_sample {
	u32	t;	/* time measurement was taken */
	u32	v;	/* value measured */
};

/* State for the parameterized min-max tracker */
struct minmax {
	struct minmax_sample s[3];
};

static inline u32 minmax_get(const struct minmax *m)
{
	return m->s[0].v;
}

static inline u32 minmax_reset(struct minmax *m, u32 t, u32 meas)
{
	struct minmax_sample val = { .t = t, .v = meas };

	m->s[2] = m->s[1] = m->s[0] = val;
	return m->s[0].v;
}

u32 minmax_running_max(struct minmax *m, u32 win, u32 t, u32 meas);
u32 minmax_running_min(struct minmax *m, u32 win, u32 t, u32 meas);

#endif


/* As time advances, update the 1st, 2nd, and 3rd choices. */
static u32 minmax_subwin_update(struct minmax *m, u32 win,
				const struct minmax_sample *val)
{
	u32 dt = val->t - m->s[0].t;

	if (unlikely(dt > win)) {
		/*
		 * Passed entire window without a new val so make 2nd
		 * choice the new val & 3rd choice the new 2nd choice.
		 * we may have to iterate this since our 2nd choice
		 * may also be outside the window (we checked on entry
		 * that the third choice was in the window).
		 */
		m->s[0] = m->s[1];
		m->s[1] = m->s[2];
		m->s[2] = *val;
		if (unlikely(val->t - m->s[0].t > win)) {
			m->s[0] = m->s[1];
			m->s[1] = m->s[2];
			m->s[2] = *val;
		}
	} else if (unlikely(m->s[1].t == m->s[0].t) && dt > win/4) {
		/*
		 * We've passed a quarter of the window without a new val
		 * so take a 2nd choice from the 2nd quarter of the window.
		 */
		m->s[2] = m->s[1] = *val;
	} else if (unlikely(m->s[2].t == m->s[1].t) && dt > win/2) {
		/*
		 * We've passed half the window without finding a new val
		 * so take a 3rd choice from the last half of the window
		 */
		m->s[2] = *val;
	}
	return m->s[0].v;
}

/* Check if new measurement updates the 1st, 2nd or 3rd choice max. */
u32 minmax_running_max(struct minmax *m, u32 win, u32 t, u32 meas)
{
	struct minmax_sample val = { .t = t, .v = meas };

	if (unlikely(val.v >= m->s[0].v) ||	  /* found new max? */
	    unlikely(val.t - m->s[2].t > win))	  /* nothing left in window? */
		return minmax_reset(m, t, meas);  /* forget earlier samples */

	if (unlikely(val.v >= m->s[1].v))
		m->s[2] = m->s[1] = val;
	else if (unlikely(val.v >= m->s[2].v))
		m->s[2] = val;

	return minmax_subwin_update(m, win, &val);
}
// EXPORT_SYMBOL(minmax_running_max);

/* Check if new measurement updates the 1st, 2nd or 3rd choice min. */
u32 minmax_running_min(struct minmax *m, u32 win, u32 t, u32 meas)
{
	struct minmax_sample val = { .t = t, .v = meas };

	if (unlikely(val.v <= m->s[0].v) ||	  /* found new min? */
	    unlikely(val.t - m->s[2].t > win))	  /* nothing left in window? */
		return minmax_reset(m, t, meas);  /* forget earlier samples */

	if (unlikely(val.v <= m->s[1].v))
		m->s[2] = m->s[1] = val;
	else if (unlikely(val.v <= m->s[2].v))
		m->s[2] = val;

	return minmax_subwin_update(m, win, &val);
}


struct tcp_sock {
/* inet_connection_sock has to be the first member of tcp_sock */
//	struct inet_connection_sock	inet_conn;
	int	sk_state;
// 	int	tcp_header_len;	/* Bytes of tcp header to send		*/

/*
 *	Header prediction flags
 *	0x5?10 << 16 + snd_wnd in net byte order
 */
//	__u32	pred_flags;

/*
 *	RFC793 variables by their proper names. This means you can
 *	read the code and the spec side by side (and laugh ...)
 *	See RFC793 and RFC1122. The RFC writes these in capitals.
 */
// 	__u32	rcv_nxt;	/* What we want to receive next 	*/
 	__u32	snd_nxt;	/* Next sequence we send		*/

 	__u32	snd_una;	/* First byte we want an ack for	*/
// 	__u32	snd_sml;	/* Last byte of the most recently transmitted small packet */
//	__u32	rcv_tstamp;	/* timestamp of last received ACK (for keepalives) */
//	__u32	lsndtime;	/* timestamp of last sent data packet (for restart window) */

	/* Data for direct copy to user */
//	struct {
//		struct sk_buff_head	prequeue;
//		struct task_struct	*task;
//		struct iovec		*iov;
//		int			memory;
//		int			len;
//	} ucopy;

//	__u32	snd_wl1;	/* Sequence for window update		*/
//	__u32	snd_wnd;	/* The window we expect to receive	*/
//	__u32	max_window;	/* Maximal window ever seen from peer	*/
//	__u32	pmtu_cookie;	/* Last pmtu seen by socket		*/
	__u32	mss_cache;	/* Cached effective mss, not including SACKS */
//	__u16	xmit_size_goal;	/* Goal for segmenting output packets	*/
//	__u16	ext_header_len;	/* Network protocol overhead (IP/IPv6 options) */
//
//	__u32	window_clamp;	/* Maximal window to advertise		*/
//	__u32	rcv_ssthresh;	/* Current window clamp			*/
//
//	__u32	frto_highmark;	/* snd_nxt when RTO occurred */
//	__u8	reordering;	/* Packet reordering metric.		*/
//	__u8	frto_counter;	/* Number of new acks after RTO */
//	__u8	nonagle;	/* Disable Nagle algorithm?             */
//	__u8	keepalive_probes; /* num of allowed keep alive probes	*/

/* RTT measurement */
	u64	tcp_mstamp;	/* most recent packet received/sent */
	u64	srtt_us;	/* smoothed round trip time << 3 in usecs */
	u32	mdev_us;	/* medium deviation			*/
	u32	mdev_max_us;	/* maximal mdev for the last rtt period	*/
	u32	rttvar_us;	/* smoothed mdev_max			*/
	u32	rtt_seq;	/* sequence number to update rttvar	*/
	struct  minmax rtt_min;

	__u32	packets_out;	/* Packets which are "in flight"	*/
	__u32	left_out;	/* Packets which leaved network	*/
	__u32	retrans_out;	/* Retransmitted packets out		*/
/*
 *      Options received (usually on last packet, some only on SYN packets).
 */
	struct tcp_options_received rx_opt;

/*
 *	Slow start and congestion control (see also Nagle, and Karn & Partridge)
 */
 	__u32	snd_ssthresh;	/* Slow start size threshold		*/
	__u32	snd_cwnd;	/* Sending congestion window		*/
 	__u16	snd_cwnd_cnt;	/* Linear increase counter		*/
	__u16	snd_cwnd_clamp; /* Do not allow snd_cwnd to grow above this */
//	__u32	snd_cwnd_used;
	__u32	snd_cwnd_stamp;
	__u32	bytes_acked;
	u32	lost;		/* Total data packets lost incl. rexmits */
	u32	app_limited;	/* limited until "delivered" reaches this val */
	u64	first_tx_mstamp;  /* start of window send phase */
	u64	delivered_mstamp; /* time we reached "delivered" */
	u8 rate_app_limited:1;  /* rate_{delivered,interval_us} limited? */
	u32	rate_delivered;    /* saved rate sample: packets delivered */
	u32	rate_interval_us;  /* saved rate sample: time elapsed */
	u32			sk_pacing_status; /* see enum sk_pacing */
	unsigned long		sk_pacing_rate; /* bytes per second */
	unsigned long		sk_max_pacing_rate;
	u8			sk_pacing_shift;
	u32	delivered;	/* Total data packets delivered incl. rexmits */

	u64	tcp_wstamp_ns;	/* departure time for next sent data packet */
	u64	tcp_clock_cache; /* cache last tcp_clock_ns() (see tcp_mstamp_refresh()) */

	// Ameya's variables.
	u64 send_timestamp_us;
	u64 intersend_time;

//
//	struct sk_buff_head	out_of_order_queue; /* Out of order segments go here */
//
//	struct tcp_func		*af_specific;	/* Operations which are AF_INET{4,6} specific	*/
//
 //	__u32	rcv_wnd;	/* Current receiver window		*/
//	__u32	rcv_wup;	/* rcv_nxt on last window update sent	*/
//	__u32	write_seq;	/* Tail(+1) of data held in tcp send buffer */
//	__u32	pushed_seq;	/* Last pushed seq, required to talk to windows */
//	__u32	copied_seq;	/* Head of yet unread data		*/
//
/*	SACKs data	*/
//	struct tcp_sack_block duplicate_sack[1]; /* D-SACK block */
//	struct tcp_sack_block selective_acks[4]; /* The SACKS themselves*/

//	__u16	advmss;		/* Advertised MSS			*/
//	__u16	prior_ssthresh; /* ssthresh saved at recovery start	*/
	__u32	lost_out;	/* Lost packets			*/
	__u32	sacked_out;	/* SACK'd packets			*/
	__u32	fackets_out;	/* FACK'd packets			*/
//	__u32	high_seq;	/* snd_nxt at onset of congestion	*/
//
//	__u32	retrans_stamp;	/* Timestamp of the last retransmit,
//				 * also used in SYN-SENT to remember stamp of
//				 * the first SYN. */
//	__u32	undo_marker;	/* tracking retrans started here. */
//	int	undo_retrans;	/* number of undoable retransmissions. */
//	__u32	urg_seq;	/* Seq of received urgent pointer */
//	__u16	urg_data;	/* Saved octet of OOB data and control flags */
//	__u8	urg_mode;	/* In urgent mode		*/
//	__u8	ecn_flags;	/* ECN status bits.			*/
//	__u32	snd_up;		/* Urgent pointer		*/
//
//	__u32	total_retrans;	/* Total retransmits for entire connection */
//
//	unsigned int		keepalive_time;	  /* time before keep alive takes place */
//	unsigned int		keepalive_intvl;  /* time interval between keep alive probes */
//	int			linger2;
//
//	unsigned long last_synq_overflow; 
//
/* Receiver side RTT estimation */
//	struct {
//		__u32	rtt;
//		__u32	seq;
//		__u32	time;
//	} rcv_rtt_est;

/* Receiver queue space */
//	struct {
//		int	space;
//		__u32	seq;
//		__u32	time;
//	} rcvq_space;
	struct tcp_congestion_ops *icsk_ca_ops;
	__u8			  icsk_ca_state;
	u32			  icsk_ca_priv[16];
#define ICSK_CA_PRIV_SIZE	(16 * sizeof(u32))
};

#define TCP_SKB_CB(__skb)	((struct tcp_skb_cb *)&((__skb)->cb))

struct tcp_skb_cb {
	__u32		seq;		/* Starting sequence number	*/
	__u32		end_seq;	/* SEQ + FIN + SYN + datalen	*/
	__u8		tcp_flags;	/* TCP header flags. (tcp[13])	*/

	__u8		sacked;		/* State flags for SACK.	*/
	__u8		ip_dsfield;	/* IPv4 tos or IPv6 dsfield	*/
	__u8		txstamp_ack:1,	/* Record TX timestamp for ack? */
			eor:1,		/* Is skb MSG_EOR marked? */
			has_rxtstamp:1,	/* SKB has a RX timestamp	*/
			unused:5;
	__u32		ack_seq;	/* Sequence number ACK'd	*/

	struct {
		/* There is space for up to 24 bytes */
		__u32 in_flight:30,/* Bytes in flight at transmit */
				is_app_limited:1, /* cwnd not fully used? */
				unused:1;
		/* pkts S/ACKed so far upon tx of skb, incl retrans: */
		__u32 delivered;
		/* start of send pipeline phase */
		u64 first_tx_mstamp;
		/* when we reached the "delivered" count */
		u64 delivered_mstamp;
	} tx;   /* only used for outgoing skbs */
};

struct sk_buff {
	struct tcp_skb_cb cb;
	u64		skb_mstamp_ns; /* earliest departure time */
};


static inline u32 tcp_time_stamp(const struct tcp_sock *tp)
{
	return (u64) (tp->tcp_mstamp) / (USEC_PER_SEC / TCP_TS_HZ);
}

static inline u32 tcp_stamp_us_delta(u64 t1, u64 t0)
{
	return max_t(s64, t1 - t0, 0);
}

static inline u32 tcp_skb_timestamp(const struct sk_buff *skb)
{
	return tcp_ns_to_ts(skb->skb_mstamp_ns);
}

/* provide the departure time in us unit */
static inline u64 tcp_skb_timestamp_us(const struct sk_buff *skb)
{
	return ((u64) skb->skb_mstamp_ns) / NSEC_PER_USEC;
}



static inline unsigned int tcp_left_out(const struct tcp_sock *tp)
{
	return tp->sacked_out + tp->lost_out;
}

static inline unsigned int tcp_packets_in_flight(const struct tcp_sock *tp)
{
	return tp->packets_out - tcp_left_out(tp) + tp->retrans_out;
}


extern struct tcp_congestion_ops tcp_init_congestion_ops;

struct rate_sample {
	u64  prior_mstamp; /* starting timestamp for interval */
	u32  prior_delivered;	/* tp->delivered at "prior_mstamp" */
	s32  delivered;		/* number of packets delivered over interval */
	long interval_us;	/* time for tp->delivered to incr "delivered" */
	u32 snd_interval_us;	/* snd interval for delivered packets */
	u32 rcv_interval_us;	/* rcv interval for delivered packets */
	long rtt_us;		/* RTT of last (S)ACKed packet (or -1) */
	int  losses;		/* number of packets marked lost upon ACK */
	u32  acked_sacked;	/* number of packets newly (S)ACKed upon ACK */
	u32  prior_in_flight;	/* in flight before this ACK */
	bool is_app_limited;	/* is sample from packet with bubble in pipe? */
	bool is_retrans;	/* is sample from retransmission? */
	bool is_ack_delayed;	/* is this (likely) a delayed ACK? */
};

enum sk_pacing {
	SK_PACING_NONE		= 0,
	SK_PACING_NEEDED	= 1,
	SK_PACING_FQ		= 2,
};

enum tcp_ca_state
{
	TCP_CA_Open = 0,
#define TCPF_CA_Open	(1<<TCP_CA_Open)
	TCP_CA_Disorder = 1,
#define TCPF_CA_Disorder (1<<TCP_CA_Disorder)
	TCP_CA_CWR = 2,
#define TCPF_CA_CWR	(1<<TCP_CA_CWR)
	TCP_CA_Recovery = 3,
#define TCPF_CA_Recovery (1<<TCP_CA_Recovery)
	TCP_CA_Loss = 4
#define TCPF_CA_Loss	(1<<TCP_CA_Loss)
};

#define GSO_MAX_SIZE		65536
	unsigned int		gso_max_size;
#define GSO_MAX_SEGS		65535
	u16			gso_max_segs;
#define MAX_HEADER 128

#define FLAG_ECE 1
#define FLAG_DATA_SACKED 2
#define FLAG_DATA_ACKED 4
#define FLAG_DATA_LOST 8
#define FLAG_CA_ALERT           (FLAG_DATA_SACKED|FLAG_ECE)
#define FLAG_NOT_DUP		(FLAG_DATA_ACKED)

#define FLAG_UNSURE_TSTAMP 16

#define CONFIG_DEFAULT_TCP_CONG "reno"
#define TCP_CLOSE 0

#define MAX_TCP_HEADER	(128 + MAX_HEADER)
#define MAX_TCP_OPTION_SPACE 40
#define TCP_MIN_SND_MSS		48
#define TCP_MIN_GSO_SIZE	(TCP_MIN_SND_MSS - MAX_TCP_OPTION_SPACE)

/*
 * Never offer a window over 32767 without using window scaling. Some
 * poor stacks do signed 16bit maths!
 */
#define MAX_TCP_WINDOW		32767U

/* Minimal accepted MSS. It is (60+60+8) - (20+20). */
#define TCP_MIN_MSS		88U

/* The initial MTU to use for probing */
#define TCP_BASE_MSS		1024

/* probing interval, default to 10 minutes as per RFC4821 */
#define TCP_PROBE_INTERVAL	600

/* Specify interval when tcp mtu probing will stop */
#define TCP_PROBE_THRESHOLD	8

/* After receiving this amount of duplicate ACKs fast retransmit starts. */
#define TCP_FASTRETRANS_THRESH 3

/* Maximal number of ACKs sent quickly to accelerate slow-start. */
#define TCP_MAX_QUICKACKS	16U

/* Maximal number of window scale according to RFC1323 */
#define TCP_MAX_WSCALE		14U

/* urg_data states */
#define TCP_URG_VALID	0x0100
#define TCP_URG_NOTYET	0x0200
#define TCP_URG_READ	0x0400

#define TCP_RETR1	3	/*
				 * This is how many retries it does before it
				 * tries to figure out if the gateway is
				 * down. Minimal RFC value is 3; it corresponds
				 * to ~3sec-8min depending on RTO.
				 */

#define TCP_RETR2	15	/*
				 * This should take at least
				 * 90 minutes to time out.
				 * RFC1122 says that the limit is 100 sec.
				 * 15 is ~13-30min depending on RTO.
				 */

#define TCP_SYN_RETRIES	 6	/* This is how many retries are done
				 * when active opening a connection.
				 * RFC1122 says the minimum retry MUST
				 * be at least 180secs.  Nevertheless
				 * this value is corresponding to
				 * 63secs of retransmission with the
				 * current initial RTO.
				 */

#define TCP_SYNACK_RETRIES 5	/* This is how may retries are done
				 * when passive opening a connection.
				 * This is corresponding to 31secs of
				 * retransmission with the current
				 * initial RTO.
				 */

#define TCP_TIMEWAIT_LEN (60*HZ) /* how long to wait to destroy TIME-WAIT
				  * state, about 60 seconds	*/
#define TCP_FIN_TIMEOUT	TCP_TIMEWAIT_LEN
                                 /* BSD style FIN_WAIT2 deadlock breaker.
				  * It used to be 3min, new value is 60sec,
				  * to combine FIN-WAIT-2 timeout with
				  * TIME-WAIT timer.
				  */

#define TCP_DELACK_MAX	((unsigned)(HZ/5))	/* maximal time to delay before sending an ACK */
#if HZ >= 100
#define TCP_DELACK_MIN	((unsigned)(HZ/25))	/* minimal time to delay before sending an ACK */
#define TCP_ATO_MIN	((unsigned)(HZ/25))
#else
#define TCP_DELACK_MIN	4U
#define TCP_ATO_MIN	4U
#endif
#define TCP_RTO_MAX	((unsigned)(120*HZ))
#define TCP_RTO_MIN	((unsigned)(HZ/5))
#define TCP_TIMEOUT_MIN	(2U) /* Min timeout for TCP timers in jiffies */
#define TCP_TIMEOUT_INIT ((unsigned)(1*HZ))	/* RFC6298 2.1 initial RTO value	*/
#define TCP_TIMEOUT_FALLBACK ((unsigned)(3*HZ))	/* RFC 1122 initial RTO value, now
						 * used as a fallback RTO for the
						 * initial data transmission if no
						 * valid RTT sample has been acquired,
						 * most likely due to retrans in 3WHS.
						 */

#define TCP_RESOURCE_PROBE_INTERVAL ((unsigned)(HZ/2U)) /* Maximal interval between probes
					                 * for local resources.
					                 */
#define TCP_KEEPALIVE_TIME	(120*60*HZ)	/* two hours */
#define TCP_KEEPALIVE_PROBES	9		/* Max of 9 keepalive probes	*/
#define TCP_KEEPALIVE_INTVL	(75*HZ)

#define MAX_TCP_KEEPIDLE	32767
#define MAX_TCP_KEEPINTVL	32767
#define MAX_TCP_KEEPCNT		127
#define MAX_TCP_SYNCNT		127

#define TCP_SYNQ_INTERVAL	(HZ/5)	/* Period of SYNACK timer */

#define TCP_PAWS_24DAYS	(60 * 60 * 24 * 24)
#define TCP_PAWS_MSL	60		/* Per-host timestamps are invalidated
					 * after this time. It should be equal
					 * (or greater than) TCP_TIMEWAIT_LEN
					 * to provide reliability equal to one
					 * provided by timewait state.
					 */
#define TCP_PAWS_WINDOW	1		/* Replay window for per-host
					 * timestamps. It must be less than
					 * minimal timewait lifetime.
					 */
/*
 *	TCP option
 */

#define TCPOPT_NOP		1	/* Padding */
#define TCPOPT_EOL		0	/* End of options */
#define TCPOPT_MSS		2	/* Segment size negotiating */
#define TCPOPT_WINDOW		3	/* Window scaling */
#define TCPOPT_SACK_PERM        4       /* SACK Permitted */
#define TCPOPT_SACK             5       /* SACK Block */
#define TCPOPT_TIMESTAMP	8	/* Better RTT estimations/PAWS */
#define TCPOPT_MD5SIG		19	/* MD5 Signature (RFC2385) */
#define TCPOPT_FASTOPEN		34	/* Fast open (RFC7413) */
#define TCPOPT_EXP		254	/* Experimental */
/* Magic number to be after the option value for sharing TCP
 * experimental options. See draft-ietf-tcpm-experimental-options-00.txt
 */
#define TCPOPT_FASTOPEN_MAGIC	0xF989
#define TCPOPT_SMC_MAGIC	0xE2D4C3D9

/*
 *     TCP option lengths
 */

#define TCPOLEN_MSS            4
#define TCPOLEN_WINDOW         3
#define TCPOLEN_SACK_PERM      2
#define TCPOLEN_TIMESTAMP      10
#define TCPOLEN_MD5SIG         18
#define TCPOLEN_FASTOPEN_BASE  2
#define TCPOLEN_EXP_FASTOPEN_BASE  4
#define TCPOLEN_EXP_SMC_BASE   6

/* But this is what stacks really send out. */
#define TCPOLEN_TSTAMP_ALIGNED		12
#define TCPOLEN_WSCALE_ALIGNED		4
#define TCPOLEN_SACKPERM_ALIGNED	4
#define TCPOLEN_SACK_BASE		2
#define TCPOLEN_SACK_BASE_ALIGNED	4
#define TCPOLEN_SACK_PERBLOCK		8
#define TCPOLEN_MD5SIG_ALIGNED		20
#define TCPOLEN_MSS_ALIGNED		4
#define TCPOLEN_EXP_SMC_BASE_ALIGNED	8

/* Flags in tp->nonagle */
#define TCP_NAGLE_OFF		1	/* Nagle's algo is disabled */
#define TCP_NAGLE_CORK		2	/* Socket is corked	    */
#define TCP_NAGLE_PUSH		4	/* Cork is overridden for already queued data */

/* TCP thin-stream limits */
#define TCP_THIN_LINEAR_RETRIES 6       /* After 6 linear retries, do exp. backoff */

/* TCP initial congestion window as per rfc6928 */
#define TCP_INIT_CWND		10

/* Bit Flags for sysctl_tcp_fastopen */
#define	TFO_CLIENT_ENABLE	1
#define	TFO_SERVER_ENABLE	2
#define	TFO_CLIENT_NO_COOKIE	4	/* Data in SYN w/o cookie option */

/* Accept SYN data w/o any cookie option */
#define	TFO_SERVER_COOKIE_NOT_REQD	0x200

/* Force enable TFO on all listeners, i.e., not requiring the
 * TCP_FASTOPEN socket option.
 */
#define	TFO_SERVER_WO_SOCKOPT1	0x400


static inline u32 tcp_min_rtt(const struct tcp_sock *tp)
{
	return minmax_get(&tp->rtt_min);
}


static inline bool before(__u32 seq1, __u32 seq2)
{
        return (s32)(seq1-seq2) < 0;
}

// static inline u32 prandom_u32_max(u32 ep_ro)
// {
// 	return (u32)(((u64) prandom_u32() * ep_ro) >> 32);
// }

#endif