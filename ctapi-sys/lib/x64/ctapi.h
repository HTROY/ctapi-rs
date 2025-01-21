/*hdr
**
**	Copyright Schneider Electric (Australia) Pty Limited
**
**	FILE NAME:	ctapi.h
**
**	AUTHOR:		Martin Roberts
**
**	DATE:		November 1996
**
**	FILE DESCRIPTION:
** 		Citect API functions.
**
**	FUNCTIONS:
**
**
**	NOTES:
**		All functions which are exported for use by external
**		programs MUST use the CTAPICALL convention, so that
**		they can be accessed by VB and Delphi Programs.
**
**
*/

#ifndef _CTAPI_H_
#define _CTAPI_H_

/************** SYSTEM INCLUDE FILES ************************************/

/************** USER INCLUDE FILES **************************************/

/************** DEFINES *************************************************/

#define ERROR_USER_DEFINED_BASE     0x10000000			/* bit 29 set			*/
#define	CT_TO_WIN32_ERROR(dwStatus) ((dwStatus) + ERROR_USER_DEFINED_BASE)
#define	WIN32_TO_CT_ERROR(dwStatus) ((dwStatus) - ERROR_USER_DEFINED_BASE)
#define	IsCitectError(dwStatus)	    (ERROR_USER_DEFINED_BASE < dwStatus)

#define ctHasOverlappedIoCompleted(pctOverlapped)	((pctOverlapped)->dwStatus != STATUS_PENDING)

#define	CT_SCALE_RANGE_CHECK	0x00000001	/* range check the variable	*/
#define	CT_SCALE_CLAMP_LIMIT	0x00000002	/* clamp variable at limits	*/
#define	CT_SCALE_NOISE_FACTOR	0x00000004	/* noise factor	on limits	*/

#define	CT_FMT_NO_SCALE		0x00000001	/* don't scale the variable	*/
#define	CT_FMT_NO_FORMAT	0x00000002	/* don't apply format		*/
#define	CT_FMT_LAST		0x00000004	/* get last value of data	*/
#define	CT_FMT_RANGE_CHECK	0x00000008	/* range check the variable	*/


#define	CT_FIND_SCROLL_NEXT	0x00000001	/* scroll to next record	*/
#define	CT_FIND_SCROLL_PREV	0x00000002	/* scroll to prev record	*/
#define	CT_FIND_SCROLL_FIRST	0x00000003	/* scroll to first record	*/
#define	CT_FIND_SCROLL_LAST	0x00000004	/* scroll to last record	*/
#define	CT_FIND_SCROLL_ABSOLUTE	0x00000005	/* scroll to absolute record	*/
#define	CT_FIND_SCROLL_RELATIVE	0x00000006	/* scroll to relative record	*/

#define	CT_OPEN_CRYPT		0x00000001	/* use encryption		*/
#define	CT_OPEN_RECONNECT	0x00000002	/* reconnect on failure		*/
#define	CT_OPEN_READ_ONLY	0x00000004	/* read only mode		*/
#define	CT_OPEN_BATCH		0x00000008	/* batch mode			*/

#define	CT_LIST_EVENT		0x00000001	/* list event mode		*/
#define	CT_LIST_LIGHTWEIGHT_MODE	0x00000002	/* list lightweight mode		*/

#define	CT_LIST_EVENT_NEW	0x00000001	/* get event for new tags	*/
#define	CT_LIST_EVENT_STATUS	0x00000002	/* get events for status change	*/

#define	CT_LIST_VALUE							0x00000001		/* value						*/
#define	CT_LIST_TIMESTAMP						0x00000002		/* timestamp					*/
#define	CT_LIST_VALUE_TIMESTAMP					0x00000003		/* valueTimestamp				*/
#define	CT_LIST_QUALITY_TIMESTAMP				0x00000004		/* qualityTimestamp				*/
#define	CT_LIST_QUALITY_GENERAL					0x00000005		/* quality general				*/
#define	CT_LIST_QUALITY_SUBSTATUS				0x00000006		/* quality substatus			*/
#define	CT_LIST_QUALITY_LIMIT					0x00000007		/* quality limit				*/
#define	CT_LIST_QUALITY_EXTENDED_SUBSTATUS		0x00000008		/* quality extended substatus	*/
#define	CT_LIST_QUALITY_DATASOURCE_ERROR		0x00000009		/* quality datasource error		*/
#define	CT_LIST_QUALITY_OVERRIDE				0x0000000A		/* quality override				*/
#define	CT_LIST_QUALITY_CONTROL_MODE			0x0000000B		/* quality control mode			*/


#define	PROPERTY_NAME_LEN	256		/* property name length		*/

#define CTAPICALL		__stdcall


/************** TYPEDEFS ************************************************/

#pragma pack(push, 1)

/*
**	CTOVERLAPPED structure.
**
**	This structure is used to control of overlapped operations. This
**	allows a client to wait on an event or poll to find when an
**	operation completes. This structure is made to look like the
**	standard WIN32 OVERLAPPED structure.
**
*/

typedef	struct
{
	DWORD		dwStatus;	/* completion status		*/
	DWORD		dwLength;	/* length of result buffer	*/
	BYTE*		pData;		/* result buffer		*/
	DWORD		OffsetHigh;	/* not used (as per Win32)	*/
	HANDLE		hEvent;		/* event handle to signal	*/
}	CTOVERLAPPED;


/*	CT_TAGVALUE_ITEMS structure
**
**	This structure holds the tag element items.
**
*/

typedef	struct
{
	DWORD					dwLength;							/* size, in bytes, of this structure	*/
	unsigned __int64		nTimestamp;							/*	timestamp							*/
	unsigned __int64		nValueTimestamp;					/*	value timestamp						*/
	unsigned __int64		nQualityTimestamp;					/*	quality timestamp					*/
	BYTE					bQualityGeneral;					/*	quality general						*/
	BYTE					bQualitySubstatus;					/*	quality substatus					*/
	BYTE					bQualityLimit;						/*	quality limit						*/
	BYTE					bQualityExtendedSubstatus;			/*	quality extended substatus			*/
	UINT					nQualityDatasourceErrorCode;		/*	quality datasource error			*/
	BOOLEAN					bOverride;							/*	quality override flag				*/
	BOOLEAN					bControlMode;						/*	quality control mode flag			*/
}	CT_TAGVALUE_ITEMS;


/*	CTSCALE structure
**
**	This structure holds the scaling values to calculate the scaling
**	of variable tags.
**
*/

typedef	struct
{
	double		dZero;		/* zero scale			*/
	double		dFull;		/* full scale			*/
}	CTHSCALE;

typedef	struct
{
	CTHSCALE	Raw;		/* raw scales			*/
	CTHSCALE	Eng;		/* engineering scales		*/
}	CTSCALE;

typedef	struct
{
	char		szName[PROPERTY_NAME_LEN];
	DWORD		dwType;
}	META_PROPERTY;

#pragma pack(pop)

// DBTYPEENUM copied from oledb.h which is part of the oledb SDK
#ifndef __oledb_h__
#define __oledb_h__

enum DBTYPEENUM 
{	DBTYPE_EMPTY		= 0,
	DBTYPE_NULL		= 1,
	DBTYPE_I2		= 2,
	DBTYPE_I4		= 3,
	DBTYPE_R4		= 4,
	DBTYPE_R8		= 5,
	DBTYPE_CY		= 6,
	DBTYPE_DATE		= 7,
	DBTYPE_BSTR		= 8,
	DBTYPE_IDISPATCH	= 9,
	DBTYPE_ERROR		= 10,
	DBTYPE_BOOL		= 11,
	DBTYPE_VARIANT		= 12,
	DBTYPE_IUNKNOWN		= 13,
	DBTYPE_DECIMAL		= 14,
	DBTYPE_UI1		= 17,
	DBTYPE_ARRAY		= 0x2000,
	DBTYPE_BYREF		= 0x4000,
	DBTYPE_I1		= 16,
	DBTYPE_UI2		= 18,
	DBTYPE_UI4		= 19,
	DBTYPE_I8		= 20,
	DBTYPE_UI8		= 21,
	DBTYPE_GUID		= 72,
	DBTYPE_VECTOR		= 0x1000,
	DBTYPE_RESERVED		= 0x8000,
	DBTYPE_BYTES		= 128,
	DBTYPE_STR		= 129,
	DBTYPE_WSTR		= 130,
	DBTYPE_NUMERIC		= 131,
	DBTYPE_UDT		= 132,
	DBTYPE_DBDATE		= 133,
	DBTYPE_DBTIME		= 134,
	DBTYPE_DBTIMESTAMP	= 135
};

#endif //#ifndef __oledb_h__


/************** ENTRY POINT DECLARATIONS ********************************/

#ifdef __cplusplus
extern "C" {
#endif

extern	HANDLE	CTAPICALL	ctOpen(LPCSTR,LPCSTR,LPCSTR,DWORD);				/* Open CTAPI interface		*/
extern	BOOL	CTAPICALL	ctOpenEx(LPCSTR,LPCSTR,LPCSTR,DWORD,HANDLE);
extern	HANDLE	CTAPICALL	ctClientCreate();
extern	BOOL	CTAPICALL	ctClientDestroy(HANDLE);
extern	BOOL	CTAPICALL	ctClose(HANDLE);						/* Close CTAPI interface	*/
extern	BOOL	CTAPICALL	ctCloseEx(HANDLE, BOOL);
extern	BOOL	CTAPICALL	ctCancelIO(HANDLE,CTOVERLAPPED*);				/* cancel pending I/O		*/
extern	DWORD	CTAPICALL	ctCicode(HANDLE,LPCSTR,DWORD,DWORD,LPSTR,DWORD,CTOVERLAPPED*);	/* execute cicode		*/
extern	BOOL	CTAPICALL	ctPointWrite(HANDLE,HANDLE,void*,DWORD,CTOVERLAPPED*);		/* write to point handle	*/
extern	BOOL	CTAPICALL	ctPointRead(HANDLE,HANDLE,void*,DWORD,CTOVERLAPPED*);		/* read from point handle	*/
extern	HANDLE	CTAPICALL	ctTagToPoint(HANDLE,LPCSTR,DWORD,CTOVERLAPPED*);		/* convert tag into point handle*/
extern	BOOL	CTAPICALL	ctPointClose(HANDLE,HANDLE);					/* free a point handle		*/
extern	HANDLE	CTAPICALL	ctPointCopy(HANDLE);						/* copy a point handle		*/
extern	BOOL	CTAPICALL	ctPointGetProperty(HANDLE,LPCTSTR,void*,DWORD,DWORD*,DWORD);	/* get point property		*/
extern	DWORD	CTAPICALL	ctPointDataSize(HANDLE);					/* size of point data buffer	*/
extern	DWORD	CTAPICALL	ctPointBitShift(HANDLE);					/* calculate bit shift offset	*/
extern	BOOL	CTAPICALL	ctPointToStr(HANDLE,BYTE*,DWORD,BYTE*,DWORD,DWORD);		/* format point data to string	*/
extern	BOOL	CTAPICALL	ctStrToPoint(HANDLE,LPCSTR,DWORD,BYTE*,DWORD,DWORD);		/* format string data into point*/
extern	BOOL	CTAPICALL	ctTagWrite(HANDLE,LPCSTR,LPCSTR);					/* write to tag				*/
extern	BOOL	CTAPICALL	ctTagWriteEx(HANDLE,LPCSTR,LPCSTR,CTOVERLAPPED*);	/* write to tag	overlaped	*/
extern	BOOL	CTAPICALL	ctTagRead(HANDLE,LPCSTR,LPSTR,DWORD);				/* read from tag			*/
extern	BOOL	CTAPICALL	ctTagReadEx(HANDLE,LPCSTR,LPSTR,DWORD,CT_TAGVALUE_ITEMS*);		/* read extended data from tag			*/
extern	BOOL	CTAPICALL	ctEngToRaw(double*,double,CTSCALE*,DWORD);			/* scale from eng to raw	*/
extern	BOOL	CTAPICALL	ctRawToEng(double*,double,CTSCALE*,DWORD);			/* scale from raw to eng	*/
extern	BOOL	CTAPICALL	ctGetOverlappedResult(HANDLE,CTOVERLAPPED*,DWORD*,BOOL);	/* get overlapped result	*/
extern	BOOL	CTAPICALL	ctEngToRaw(double*,double,CTSCALE*,DWORD);			/* scale from eng to raw	*/
extern	BOOL	CTAPICALL	ctRawToEng(double*,double,CTSCALE*,DWORD);			/* scale from raw to eng	*/
extern	HANDLE	CTAPICALL	ctFindFirst(HANDLE,LPCTSTR,LPCTSTR,HANDLE*,DWORD);		/* initiate a search		*/
extern	HANDLE	CTAPICALL	ctFindFirstEx(HANDLE,LPCTSTR,LPCTSTR,LPCTSTR,HANDLE*,DWORD);		/* initiate a search		*/
extern	BOOL	CTAPICALL	ctFindNext(HANDLE,HANDLE*);					/* get the next search item	*/
extern	BOOL	CTAPICALL	ctFindPrev(HANDLE,HANDLE*);					/* get the prev search item	*/
extern	DWORD	CTAPICALL	ctFindScroll(HANDLE,DWORD,LONG,HANDLE*);			/* scroll to search item	*/
extern	BOOL	CTAPICALL	ctFindClose(HANDLE);						/* close a search		*/
extern	LONG	CTAPICALL	ctFindNumRecords(HANDLE);					/* get the total number of records in the search */
extern	BOOL	CTAPICALL	ctGetProperty(HANDLE,LPCTSTR,void*,DWORD,DWORD*,DWORD);		/* get a named property		*/
extern	HANDLE	CTAPICALL	ctListNew(HANDLE,DWORD);					/* create poll list		*/
extern	BOOL	CTAPICALL	ctListFree(HANDLE);						/* free poll list		*/
extern	HANDLE	CTAPICALL	ctListAdd(HANDLE,LPCSTR);						/* add tag to poll list		*/
extern  HANDLE	CTAPICALL	ctListAddEx(HANDLE hList, LPCSTR sTag, BOOL bRaw, int nPollPeriodMS, double dDeadband); // add tag to poll list
extern	BOOL	CTAPICALL	ctListDelete(HANDLE);														// delete tag from poll list
extern	BOOL	CTAPICALL	ctListRead(HANDLE,CTOVERLAPPED*);											// read poll list
extern	BOOL	CTAPICALL	ctListWrite(HANDLE,LPCSTR,CTOVERLAPPED*);									// write poll list item
extern	BOOL	CTAPICALL	ctListData(HANDLE,void*,DWORD,DWORD);										// get list data
extern	HANDLE	CTAPICALL	ctListEvent(HANDLE,DWORD);													// get list event
extern	BOOL	CTAPICALL	ctListItem(HANDLE,DWORD,void*,DWORD,DWORD);									// get tag element extended data

extern	BOOL	CTAPICALL	ctTagGetProperty(HANDLE hCTAPI, LPCTSTR szTagName, LPCTSTR szProperty,
											 void* pData, DWORD dwBufferLength, DWORD dwType);			// get a tag property

#ifdef __cplusplus
}
#endif

/************** GLOBAL VARIABLE DEFINITIONS *****************************/



/************** EXTERNAL DECLARATIONS ***********************************/



/************************************************************************/

#endif //#ifndef _CTAPI_H_
