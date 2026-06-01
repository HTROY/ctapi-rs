# TrnQuery

Provides a powerful interface into the trend achive from external applications, replacing the old CtAPITrend query. TrnQuery performs significantly better than CtAPITrend.

TrnQuery is performed through the same mechanism as CtAPITrend. To establish the query and return the first record, you call ctFindFirst. Then, to browse the remaining records, you call ctFindNext. To access the data of the current record, ctGetProperty is called for each field of the record.

ctFindFirst is called with the following parameters:

hCtapi: handle to a valid Ctapi client instance.
szTableName: command string for the Trnquery, see below.
szFilter: Not used for Trnquery. Just pass in NULL.
hObject: handle to the first record retrieved for the query.
dwFlags: Not used for Trnquery. Just pass in 0.
The szTableName is the command string for the query. It contains the parameters for the query.

Syntax

TRNQUERY,Endtime,EndtimeMs,Period,NumSamples,Tagname,Displaymode,Datamode,InstantTrend,SamplePeriod'

Note: Arguments needs to be comma-separated. Spaces between arguments are supported but not necessary. We recommend no spaces between arguments as they require more processing and take up more space in the query string.

Endtime:

End time of the trend query in seconds since 1970 as an integer. This time is expected to be a UTC time (Universal Time Coordinates).

EndtimeMs:

Millisecond portion of the end time as an integer, expected to be a number between 0 and 999.

Period:

Time period in seconds between the samples returned as a floating point value. The only decimal separator supported is the `.'.

NumSamples:

Number of samples requested as an integer. The start time of the request is calculated by multiplying the Period by NumSamples - 1, then subtracting this from the EndTime.

The actual maximum amount of samples returned is actually NumSamples + 2. This is because we return the previous and next samples before and after the requested range. This is useful as it tells you where the next data is before and after where you requested it.

TagName:

The name of the trend tag as a string. This query only supports the retrieval of trend data for one trend at a time.

DisplayMode:

Specifies the different options for formatting and calculating the samples of the query as an unsigned integer. See Display Mode for information.

DataMode:

Mode of this request as an integer. 1 if you want the timestamps to be returned with their full precision and accuracy. Mode 1 does not interpolate samples where there were no values. 0 if you want the timestamps to be calculated, one per period. Mode 0 does interpolate samples, where there was no values.

InstantTrend:

An integer specifying whether the query is for an instant trend. 1 if for an instant trend. 0 if not.

SamplePeriod:

An integer specifying the requested sample period in milliseconds for the instant trend's tag value.

Return Value

See Returned Data for return values.


# Display Mode
The data returned can vary drastically depending on the display mode of the TrnQuery. The display mode is split into the following mutually exclusive options:

Ordering Trend sample options

0 - Order returned samples from oldest to newest
1 - Order returned samples from newest to oldest. This mode is not supported when the Raw data option has been specified.
Condense method options

0 - Set the condense method to use the mean of the samples.
4 - Set the condense method to use the minimum of the samples.
8 - Set the condense method to use the maximum of the samples.
12 - Set the condense method to use the newest of the samples.
Stretch method options

0 - Set the stretch method to step.
128 - Set the stretch method to use a ratio.
256 - Set the stretch method to use raw samples (no interpolation).
Gap Fill Constant option

n - the number of missed samples that the user wants to gap fill) x 4096.
Last valid value option

0 - If we are leaving the value given with a bad quality sample as 0.
2097152 - If we are to set the value of a bad quality sample to the last valid value (zero if there is no last valid value).
Raw data option

0 - If we are not returning raw data, that is we are using the condense and stretch modes to compress and interpolate the data.
4194304 - If we are to return totally raw data, that is no compression or interpolation. This mode is only supported if we have specified the DataMode of the query = 1. When using this mode, more samples than the maximum specified above will be returned if there are more raw samples than the maximum in the time range.

# Returned Data
The following properties are returned for each data record of the query.

DateTime: Time of the trend sample in seconds since 1970 as an integer in UTC (Universal Time Coordinates).
MSeconds: Millisecond component of the time of the trend sample as an integer. This value is inbetween 0 and 999.
Value: Trend value of the sample as a double.
Quality: The quality information associated with the trend sample as an unsigned integer. The Quality property contains different information in different bits of the unsigned integer as follows:
Value Type (Bits 0 - 3)

ValueType_None (0): There is no value in the given sample. Ignore the sample value, time and quality.
ValueType_Interpolated (1): The value has been interpolated from data around it.
ValueType_SingleRaw (2): The value is based on one raw sample.
ValueType_MultipleRaw (3): The value has been calculated from multiple raw samples.
Value Quality (Bits 4 - 7)

ValueQuality_Bad (0): Ignore the value of the sample as there was no raw data to base it on.
ValueQuality_Good (1): The value of the sample is valid, and is based on some raw data.
Last Value Quality (Bits 8 - 11)

LastValueQuality_Bad (0: The value of the sample should be ignored as there was no raw data to base it on.
LastValueQuality_Good (1): The value quality of the last raw sample in the period was good.
LastValueQuality_NotAvailable (2): The value quality of the last raw sample in the period was Not Available.
LastValueQuality_Gated (3): The value quality of the last raw sample in the period was Gated.
Partial Flag (Bit 12)

When the Partial Flag is set to 1 it indicates that the sample may change the next time it is read. This occurs when you get samples right at the current time, and a sample returned is not necessarily complete because more samples may be acquired in this period.