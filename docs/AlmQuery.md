# AlmQuery

Provides an interface into the alarm summary archive from external applications, replacing the old CtAPIAlarm query. AlmQuery performs significantly better than CtAPIAlarm.

Note: As part of 2018 R2 the AlmQuery function has been re-implemented to return results based on the sequence of events (SOE) page. The format of the result has not been modified and is compatible with 7.20. The re-implementation includes:

TableName : No longer supports ArgAnaAlm.
Alarm tags are unique (enforced by the compiler), so the comment on alarmTag is no longer necessary.
The comment associated with the sample is coming from the user comments on the SOE page.
AlmQuery is performed through the same mechanism as CtAPIAlarm. To establish the query and return the first record, you call ctFindFirst. Then, to browse the remaining records, you call ctFindNext. To access the data of the current record, ctGetProperty is called for each field of the record.

ctFindFirst is called with the following parameters:

hCtapi: Handle to a valid CtAPI client instance.
szTableName: Command string for the almquery, see below.
szFilter: Not used for Almquery. Just pass in NULL.
hObject: Handle to the first record retrieved for the query.
dwFlags: Not used for Almquery. Just pass in 0.
The szTableName is the command string for the query and contains the parameters for the query.

Syntax

`ALMQUERY,Database,TagName,Starttime,StarttimeMs,Endtime,EndtimeMs,Period'

Note: Arguments need to be comma-separated. Spaces between arguments are supported but not necessary. We recommend no spaces between arguments as they require more processing and take up more space in the query string.

Database:

The Alarm database that the alarm is in (alarm type). The following databases are supported: DigAlm (Digital), AnaAlm (Analog), AdvAlm (Advanced), HResAlm (Time Stamped), ArgDigAlm (Multi-Digital), TsDigAlm (Time Stamped Digital), TsAnaAlm (Timestamped Analog).

TagName:

The Alarm tag as a string.

Starttime:

The start time of the alarm query in seconds since 1970 as an integer in UTC time.

StarttimeMs:

The millisecond portion of the start time as an integer. It is expected to be a number between 0 and 999.

Endtime:

The end time of the alarm query in seconds since 1970 as an integer in UTC time.

EndtimeMs:

Millisecond portion of the end time as an integer. It is expected to be a number between 0 and 999.

Period:

Time period in seconds between the samples returned as a floating point value. The only decimal separator supported is the `.'.

Return Value

The maximum number of samples returned is the time range divided by the period, plus 3 (one for the sample exactly on the end time, and two for the previous and next samples).

Note: Divide the period evenly into the time range, otherwise one extra sample may be returned.

The AlmQuery does not return interpolated samples in periods where there were no alarm samples. However, to stay within the allowable number of samples, the raw alarm samples will be compressed when more than one sample occurs in one period.

When this compression occurs, the returned sample is flagged as a multiple sample. The timestamp is then an average of the samples within the period. The value and comment returned reflects that of the last sample in the period.

The following properties are returned for each data record of the query.

DateTime: The time of the alarm sample in seconds since 1970 as an integer. This Time is in UTC (Universal Time Coordinates).
MSeconds: The millisecond component of the time of the trend sample as an integer. This value is in between 0 and 999.
Comment: The comment associated with the alarm sample as a string. Corresponds to the latest user comment associated with the most recent event on the alarm within the current period.
Value: The alarm value of the sample as an unsigned integer. See below for a detailed description of the alarm value. The alarm value contains information describing the state of the alarm at the time of the sample:
bGood (Bit 0)- Future use only, intended to show when the quality of the alarm data goes bad. At the moment every sample has this bit set to 1 to say the sample is good.

bDisabled (Bit 1)- 1 if the alarm is disabled at the sample's time, 0 otherwise.

bMultiple (Bit 2)- 1 if the alarm sample is based on multiple raw samples, 0 if it is based on only 1 raw sample.

bOn (Bit 3)- 1 if the alarm is on at the sample's time, 0 otherwise.

bAck (Bit 4)- 1 if the alarm is acknowledged at the sample's time, 0 otherwise.

state (Bits 5 - 7)- Contains the state information of the alarm at the sample's time.

The alarm state represents the different states of the different alarm types. The state only contains relevant information if the alarm is on.

For analog, and time-stamped analog alarms the state can be as follows:

Deviation High (1)- The alarm has deviated above the Setpoint by more than the specified threshold.
Deviation Low (2)- The alarm has deviated below the Setpoint by more than the specified threshold.
Rate of Change (3)- The alarm has changed at a faster rate than expected.
Low (4)- The alarm has entered the low alarm range of values.
High (5)- The alarm has entered the high alarm range of values.
Low Low (6)- The alarm has entered the low low alarm range of values.
High High (7)- The alarm has entered the high high alarm range of values.
For Multi-Digital Alarms the state can be as follows:

000 (0)- Digital tags for the alarm are off.
00A (1)- Tag A is on, B and C are off.
0B0 (2)- Tag B is on, A and C are off.
0BA (3)- Tags B and A are on, C is off.
C00 (4)- Tag C is on, B and C are off.
C0A (5)- Tag C and A are on, B is off.
CB0 (6)- Tag C and B are on, A is off.
CBA (7)-Digital tags for the alarm are on.
For the rest of the alarm types ignore the state information.