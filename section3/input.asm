MOV R1, R0         
ADD R2, R1, R0     
SUB R3, R2, R1      


MUL R4, R2, R1    


AND R5, R3, R2     
ORR R6, R4, R3     

LDR R7, [R6, #4]   
STR R7, [R6, #8]    

B done 
done:
